use std::hash::Hash;

use ndarray::ArrayD;

use smartnoise_validator::{Float, proto};
use smartnoise_validator::base::{Array, IndexKey, Jagged, ReleaseNode, Value};
use smartnoise_validator::errors::*;
use smartnoise_validator::utilities::{standardize_categorical_argument, standardize_null_candidates_argument, standardize_numeric_argument, standardize_weight_argument, take_argument};

use crate::components::Evaluable;
use crate::NodeArguments;
use crate::utilities;
use crate::utilities::get_num_columns;
use crate::utilities::noise;

impl Evaluable for proto::Impute {
    fn evaluate(&self, privacy_definition: &Option<proto::PrivacyDefinition>, mut arguments: NodeArguments) -> Result<ReleaseNode> {

        let enforce_constant_time = privacy_definition.as_ref()
            .map(|v| v.protect_elapsed_time).unwrap_or(false);

        // if categories argument is not None, treat data as categorical (regardless of atomic type)
        if arguments.contains_key::<IndexKey>(&"categories".into()) {
            let weights = take_argument(&mut arguments, "weights")
                .and_then(|v| v.jagged()).and_then(|v| v.float()).ok();

            Ok(ReleaseNode::new(match (
                take_argument(&mut arguments, "data")?.array()?,
                take_argument(&mut arguments, "categories")?.jagged()?,
                take_argument(&mut arguments, "null_values")?.jagged()?) {

                (Array::Bool(data), Jagged::Bool(categories), Jagged::Bool(nulls)) =>
                    impute_categorical_arrayd(data, categories, weights, nulls, enforce_constant_time)?.into(),

                (Array::Float(_), Jagged::Float(_), Jagged::Float(_)) =>
                    return Err("categorical imputation over floats is not currently supported".into()),
//                        impute_categorical(&data, &categories, &weights, &nulls)?.into(),

                (Array::Int(data), Jagged::Int(categories), Jagged::Int(nulls)) =>
                    impute_categorical_arrayd(data, categories, weights, nulls, enforce_constant_time)?.into(),

                (Array::Str(data), Jagged::Str(categories), Jagged::Str(nulls)) =>
                    impute_categorical_arrayd(data, categories, weights, nulls, enforce_constant_time)?.into(),
                _ => return Err("types of data, categories, and null must be consistent and probabilities must be f64".into()),
            }))
        }
        // if categories argument is None, treat data as continuous
        else {
            // get specified data distribution for imputation -- default to Uniform if no valid distribution is provided
            let distribution = match take_argument(&mut arguments, "distribution") {
                Ok(distribution) => distribution.array()?.first_string()?,
                Err(_) => "Uniform".to_string()
            };

            match distribution.to_lowercase().as_str() {
                // if specified distribution is uniform, identify whether underlying data are of atomic type f64 or i64
                // if f64, impute uniform values
                // if i64, no need to impute (numeric imputation replaces only f64::NAN values, which are not defined for the i64 type)
                "uniform" => {
                    Ok(match (take_argument(&mut arguments, "data")?, take_argument(&mut arguments, "lower")?, take_argument(&mut arguments, "upper")?) {
                        (Value::Array(data), Value::Array(lower), Value::Array(upper)) => match (data, lower, upper) {
                            (Array::Float(data), Array::Float(lower), Array::Float(upper)) =>
                                impute_float_uniform_arrayd(data, lower, upper, enforce_constant_time)?.into(),
                            (Array::Int(data), Array::Int(_lower), Array::Int(_upper)) =>
                                // continuous integers are already non-null
                                data.into(),
                            _ => return Err("data, lower, and upper must all be the same type".into())
                        },
                        _ => return Err("data, lower, upper, shift, and scale must be ArrayND".into())
                    })
                },
                // if specified distribution is Gaussian, get necessary arguments and impute
                "gaussian" => {
                    let data = take_argument(&mut arguments, "data")?.array()?.float()?;
                    let lower = take_argument(&mut arguments, "lower")?.array()?.float()?;
                    let upper = take_argument(&mut arguments, "upper")?.array()?.float()?;
                    let scale = take_argument(&mut arguments, "scale")?.array()?.float()?;
                    let shift = take_argument(&mut arguments, "shift")?.array()?.float()?;

                    Ok(impute_float_gaussian_arrayd(data, lower, upper, shift, scale, enforce_constant_time)?.into())
                },
                _ => return Err("Distribution not supported".into())
            }.map(ReleaseNode::new)
        }
    }
}

/// Returns data with imputed values in place of `f64::NAN`.
/// Values are imputed from a uniform distribution.
///
/// # Arguments
/// * `data` - Data for which you would like to impute the `NAN` values.
/// * `lower` - Lower bound on imputation range for each column.
/// * `upper` - Upper bound on imputation range for each column.
///
/// # Return
/// Data with `NAN` values replaced with imputed values.
///
/// # Example
/// ```
/// use ndarray::prelude::*;
/// use smartnoise_runtime::components::impute::impute_float_uniform_arrayd;
/// use smartnoise_validator::Float;
///
/// let data: ArrayD<Float> = arr2(&[ [1., Float::NAN, 3., Float::NAN], [2., 2., Float::NAN, Float::NAN] ]).into_dyn();
/// let lower: ArrayD<Float> = arr1(&[0., 2., 3., 4.]).into_dyn();
/// let upper: ArrayD<Float> = arr1(&[10., 2., 5., 5.]).into_dyn();
/// let imputed = impute_float_uniform_arrayd(data, lower, upper, false);
/// # imputed.unwrap();
/// ```

pub fn impute_float_uniform_arrayd(
    mut data: ArrayD<Float>,
    lower: ArrayD<Float>, upper: ArrayD<Float>,
    enforce_constant_time: bool
) -> Result<ArrayD<Float>> {

    let num_columns = get_num_columns(&data)?;

    // iterate over the generalized columns
    data.gencolumns_mut().into_iter()
        // pair generalized columns with arguments
        .zip(standardize_numeric_argument(lower, num_columns)?.into_iter())
        .zip(standardize_numeric_argument(upper, num_columns)?.into_iter())
        // for each pairing, iterate over the cells
        .try_for_each(|((mut column, min), max)| impute_float_uniform(
            column.iter_mut(), (*min, *max), enforce_constant_time))?;

    Ok(data)
}

pub fn impute_float_uniform<'a, I: Iterator<Item=&'a mut Float>>(
    // column: &mut Vec<Float>,
    // column: &mut ndarray::ArrayBase<ndarray::ViewRepr<&mut Float>, ndarray::Ix1>,
    column: I,
    (lower, upper): (Float, Float),
    enforce_constant_time: bool
) -> Result<()> {
    column
        // ignore nan values
        .filter(|v| v.is_nan())
        // mutate the cell via the operator
        .try_for_each(|v| noise::sample_uniform(
            lower as f64, upper as f64, enforce_constant_time)
            .map(|n| *v = n as Float))
}

/// Returns data with imputed values in place of `f64::NAN`.
/// Values are imputed from a truncated Gaussian distribution.
///
/// # Arguments
/// * `data` - Data for which you would like to impute the `NAN` values.
/// * `shift` - The mean of the untruncated Gaussian noise distribution for each column.
/// * `scale` - The standard deviation of the untruncated Gaussian noise distribution for each column.
/// * `lower` - Lower bound on imputation range for each column.
/// * `upper` - Upper bound on imputation range for each column.
///
/// # Return
/// Data with `NAN` values replaced with imputed values.
///
/// # Example
/// ```
/// use ndarray::prelude::*;
/// use smartnoise_runtime::components::impute::impute_float_gaussian_arrayd;
/// use smartnoise_validator::Float;
/// let data: ArrayD<Float> = arr1(&[1., Float::NAN, 3., Float::NAN]).into_dyn();
/// let lower: ArrayD<Float> = arr1(&[0.0]).into_dyn();
/// let upper: ArrayD<Float> = arr1(&[10.0]).into_dyn();
/// let shift: ArrayD<Float> = arr1(&[5.0]).into_dyn();
/// let scale: ArrayD<Float> = arr1(&[7.0]).into_dyn();
/// let imputed = impute_float_gaussian_arrayd(data, lower, upper, shift, scale, false);
/// # imputed.unwrap();
/// ```
pub fn impute_float_gaussian_arrayd(
    mut data: ArrayD<Float>,
    lower: ArrayD<Float>, upper: ArrayD<Float>,
    shift: ArrayD<Float>, scale: ArrayD<Float>,
    enforce_constant_time: bool
) -> Result<ArrayD<Float>> {

    let num_columns = get_num_columns(&data)?;

    // iterate over the generalized columns
    data.gencolumns_mut().into_iter()
        // pair generalized columns with arguments
        .zip(standardize_numeric_argument(lower, num_columns)?.into_iter()
            .zip(standardize_numeric_argument(upper, num_columns)?.into_iter()))
        .zip(standardize_numeric_argument(shift, num_columns)?.into_iter()
            .zip(standardize_numeric_argument(scale, num_columns)?.into_iter()))
        // for each pairing, iterate over the cells
        .try_for_each(|((mut column, bounds), params)|
            impute_float_gaussian(
                column.iter_mut(),bounds,params, enforce_constant_time))?;

    Ok(data)
}

pub fn impute_float_gaussian<'a, I: Iterator<Item=&'a mut Float>>(
    // column: &mut Vec<Float>,
    column: I,
    (min, max): (&Float, &Float),
    (shift, scale): (&Float, &Float),
    enforce_constant_time: bool
) -> Result<()> {
    column
        // ignore nan values
        .filter(|v| v.is_nan())
        // mutate the cell via the operator
        .try_for_each(|v| noise::sample_gaussian_truncated(
            *min as f64, *max as f64, *shift as f64, *scale as f64,
            enforce_constant_time)
            .map(|n| *v = n as Float))
}

/// Returns data with imputed values in place on `null_value`.
///
/// # Arguments
/// * `data` - The data to be resized.
/// * `categories` - For each data column, the set of possible values for elements in the column.
/// * `weights` - For each data column, weights for each category to be used when imputing null values.
/// * `null_value` - For each data column, the value of the data to be considered NULL.
///
/// # Return
/// Data with `null_value` values replaced with imputed values.
///
/// # Example
/// ```
/// use ndarray::prelude::*;
/// use smartnoise_runtime::components::impute::impute_categorical_arrayd;
/// let data: ArrayD<String> = arr2(&[["a".to_string(), "b".to_string(), "null_3".to_string()],
///                                   ["c".to_string(), "null_2".to_string(), "a".to_string()]]).into_dyn();
/// let categories: Vec<Vec<String>> = vec![vec!["a".to_string(), "c".to_string()],
///                                         vec!["b".to_string(), "d".to_string()],
///                                         vec!["f".to_string()]];
/// let weights = Some(vec![vec![1., 1.],
///                         vec![1., 2.],
///                         vec![1.]]);
/// let null_value: Vec<Vec<String>> = vec![vec!["null_1".to_string()],
///                                         vec!["null_2".to_string()],
///                                         vec!["null_3".to_string()]];
///
/// let imputed = impute_categorical_arrayd(data, categories, weights, null_value, false);
/// # imputed.unwrap();
/// ```
pub fn impute_categorical_arrayd<T: Clone>(
    mut data: ArrayD<T>, categories: Vec<Vec<T>>,
    weights: Option<Vec<Vec<Float>>>, null_value: Vec<Vec<T>>,
    enforce_constant_time: bool
) -> Result<ArrayD<T>> where T: Clone + PartialEq + Default + Ord + Hash {

    let num_columns = get_num_columns(&data)?;

    let categories = standardize_categorical_argument(categories.to_vec(), num_columns)?;
    let lengths = categories.iter().map(|cats| cats.len() as i64).collect::<Vec<i64>>();
    let probabilities = standardize_weight_argument(&weights, &lengths)?;
    let null_value = standardize_null_candidates_argument(null_value, num_columns)?;

    // iterate over the generalized columns
    data.gencolumns_mut().into_iter()
        // pair generalized columns with arguments
        .zip(categories.iter())
        .zip(probabilities.iter())
        .zip(null_value.iter())
        // for each pairing, iterate over the cells
        .try_for_each(|(((mut column, cats), probs), null)| impute_categorical(
            column.iter_mut(), cats, probs, null, enforce_constant_time))?;

    Ok(data)
}

fn impute_categorical<'a, T: 'a, I: Iterator<Item=&'a mut T>>(
    // column: &mut Vec<T>,
    column: I,
    categories: &Vec<T>, probabilities: &Vec<Float>, null_values: &Vec<T>,
    enforce_constant_time: bool
) -> Result<()>
    where T: Clone + PartialEq + Default + Ord + Hash {
    column
        // ignore non null values
        .filter(|v| null_values.contains(v))
        // mutate the cell via the operator
        .try_for_each(|v| utilities::sample_from_set(
            &categories, &probabilities, enforce_constant_time)
            .map(|n| *v = n))
}
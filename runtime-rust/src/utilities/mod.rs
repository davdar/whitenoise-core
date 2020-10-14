use std::cmp::Ordering;

use smartnoise_validator::errors::*;
use ieee754::Ieee754;
use ndarray::{ArrayD, Axis, Zip};
use ndarray::prelude::IxDyn;
use openssl::rand::rand_bytes;

use smartnoise_validator::utilities::array::{slow_select, slow_stack};

pub mod mechanisms;
pub mod noise;

///  Accepts an ndarray and returns the number of columns.
///
/// # Arguments
/// * `data` - The data for which you want to know the number of columns.
///
/// # Return
/// Number of columns in data.
pub fn get_num_columns<T>(data: &ArrayD<T>) -> Result<i64> {
    match data.ndim() {
        0 | 1 => Ok(1),
        2 => Ok(data.len_of(Axis(1)) as i64),
        _ => Err("data may be at most 2-dimensional".into())
    }
}


/// Broadcast left and right to match each other, and map an operator over the pairs.
///
/// # Arguments
/// * `left` - Left vector to map over.
/// * `right` - Right vector to map over.
/// * `operator` - Function to apply to each pair.
///
/// # Return
/// An array of mapped data.
///
/// # Example
/// ```
/// use smartnoise_validator::errors::*;
/// use ndarray::prelude::*;
/// use smartnoise_runtime::utilities::broadcast_map;
/// let left: ArrayD<f64> = arr1(&[1., -2., 3., 5.]).into_dyn();
/// let right: ArrayD<f64> = arr1(&[2.]).into_dyn();
/// let mapped: Result<ArrayD<f64>> = broadcast_map(left, right, &|l, r| l.max(r.clone()));
/// println!("{:?}", mapped); // [2., 2., 3., 5.]
/// ```
pub fn broadcast_map<T, U>(
    left: ArrayD<T>,
    right: ArrayD<T>,
    operator: &dyn Fn(&T, &T) -> U) -> Result<ArrayD<U>> where T: std::clone::Clone, U: Default {
    let shape = match left.ndim().cmp(&right.ndim()) {
        Ordering::Less => right.shape(),
        Ordering::Equal => if left.len() > right.len() { left.shape() } else { right.shape() },
        Ordering::Greater => left.shape()
    }.to_vec();

   // println!("shape {:?}", shape);
   // println!("left shape {:?}", left.shape());
   // println!("right shape {:?}", right.shape());

    let left = to_nd(left, shape.len())?;
    let right = to_nd(right, shape.len())?;

   // println!("shape {:?}", shape);
   // println!("left shape {:?}", left.shape());
   // println!("right shape {:?}", right.shape());
   // println!();

    let mut output: ArrayD<U> = ndarray::Array::default(shape.clone());
    Zip::from(&mut output)
        .and(left.broadcast(shape.clone()).ok_or("could not broadcast left argument")?)
        .and(right.broadcast(shape).ok_or("could not broadcast right argument")?)
        .apply(|acc, l, r| *acc = operator(&l, &r));

    Ok(output)
}


#[cfg(test)]
mod test_broadcast_map {
    use ndarray::{arr0, arr1, arr2};

    use crate::utilities::broadcast_map;

    #[test]
    fn test_broadcasting() {
        let data0d = arr0(2.).into_dyn();
        let data1d = arr1(&[2., 3., 5.]).into_dyn();
        let data2d = arr2(&[[2., 4.], [3., 7.], [5., 2.]]).into_dyn();

        assert_eq!(
            broadcast_map(data0d.clone(), data1d.clone(), &|l, r| l * r).unwrap(),
            arr1(&[4., 6., 10.]).into_dyn());

        assert_eq!(
            broadcast_map(data1d.clone(), data2d.clone(), &|l, r| l / r).unwrap(),
            arr2(&[[1., 2. / 4.], [1., 3. / 7.], [1., 5. / 2.]]).into_dyn());

        assert_eq!(
            broadcast_map(data2d, data0d, &|l, r| l + r).unwrap(),
            arr2(&[[4., 6.], [5., 9.], [7., 4.]]).into_dyn());
    }

    #[test]
    fn non_conformable() {
        let left = arr1(&[2., 3., 5.]).into_dyn();
        let right = arr1(&[2., 3., 5., 6.]).into_dyn();

        assert!(broadcast_map(
            left, right, &|l, r| l * r,
        ).is_err());
    }

    #[test]
    #[should_panic]
    fn arraynd_left_broadcast() {
        // if this test doesn't panic, then ndarray has added support for left broadcasting
        // once ndarray has support for left broadcasting, then evaluate removal of broadcast_map wherever possible
        let left = arr0(2.).into_dyn();
        let right = arr1(&[2., 3., 5., 6.]).into_dyn();

        let _broadcast = left / right;
    }
}

pub fn to_nd<T>(mut array: ArrayD<T>, ndim: usize) -> Result<ArrayD<T>> {
    match (ndim as i32) - (array.ndim() as i32) {
        0 => {}
        // must remove i axes
        i if i < 0 => {
            (0..-(i as i32)).try_for_each(|_| match array.shape().last()
                .ok_or_else(|| Error::from("ndim may not be negative"))? {
                1 => {
                    array.index_axis_inplace(Axis(array.ndim() - 1), 0);
                    Ok(())
                },
                _ => Err(Error::from("cannot remove non-singleton trailing axis"))
            })?
        }
        // must add i axes
        i if i > 0 => (0..i).for_each(|_| array.insert_axis_inplace(Axis(array.ndim()))),
        _ => return Err("invalid dimensionality".into())
    };

    Ok(array)
}


pub fn standardize_columns<T: Default + Clone>(array: ArrayD<T>, column_len: usize) -> Result<ArrayD<T>> {
    Ok(match array.ndim() {
        0 => return Err("dataset may not be a scalar".into()),
        1 => match column_len {
            0 => slow_select(&array, Axis(1), &[]),
            1 => array,
            _ => slow_stack(
                Axis(1),
                &[array.view(), ndarray::Array::<T, IxDyn>::default(IxDyn(&[array.len(), column_len])).view()])?
        },
        2 => match array.len_of(Axis(1)).cmp(&column_len) {
            Ordering::Less => slow_stack(
                Axis(1),
                &[array.view(), ndarray::Array::<T, IxDyn>::default(IxDyn(&[
                    array.len_of(Axis(0)),
                    column_len - array.len_of(Axis(1))])).view()],
            )?,
            Ordering::Equal => array,
            Ordering::Greater => slow_select(&array, Axis(1), &(0..column_len).collect::<Vec<_>>())
        },
        _ => return Err("array must be 1 or 2-dimensional".into())
    })
}


/// Return bytes of binary data as `String`.
///
/// Reads bytes from OpenSSL, converts them into a string,
/// concatenates them, and returns the combined string.
///
/// # Arguments
/// * `n_bytes` - The number of random bytes you wish to read from OpenSSL.
///
/// # Return
/// The `String` representation of the bytes.
pub fn get_bytes(n_bytes: usize) -> Result<String> {
    // read random bytes from OpenSSL
    let mut buffer = vec!(0_u8; n_bytes);
    fill_bytes(&mut buffer)?;

    // create new buffer of binary representations, rather than u8
    let new_buffer = buffer.into_iter()
        .map(|v| format!("{:08b}", v))
        .collect::<Vec<String>>();

    // combine binary representations into single string and subset mantissa, and return
    Ok(new_buffer.concat())
}

// TODO: substitute implementation with different generators
pub fn fill_bytes(mut buffer: &mut [u8]) -> Result<()> {
    if let Err(e) = rand_bytes(&mut buffer) {
        Err(format!("OpenSSL Error: {}", e).into())
    } else { Ok(()) }
}


/// Converts an `f64` to `String` of length 64, yielding the IEEE-754 binary representation of the `f64`.
///
/// The first bit of the string is the sign, the next 11 are the exponent, and the last 52 are the mantissa.
/// See [here](https://en.wikipedia.org/wiki/Double-precision_floating-point_format) for an explanation
/// of IEEE-754 64-bit floating-point numbers.
///
/// # Arguments
/// * `num` - A number of type f64.
///
/// # Return
/// A string showing the IEEE-754 binary representation of `num`.
pub fn f64_to_binary(num: f64) -> String {
    // decompose num into component parts
    let (sign, exponent, mantissa) = num.decompose_raw();

    // convert each component into strings
    let sign_string = (sign as i64).to_string();
    let mantissa_string = format!("{:052b}", mantissa);
    let exponent_string = format!("{:011b}", exponent);

    // join component strings and return string representation
    vec![sign_string, exponent_string, mantissa_string].concat()
}

/// Converts `String` of length 64 to `f64`, yielding the floating-point number represented by the `String`.
///
/// The first bit of the string is the sign, the next 11 are the exponent, and the last 52 are the mantissa.
/// See [here](https://en.wikipedia.org/wiki/Double-precision_floating-point_format) for an explanation
/// of IEEE-754 64-bit floating-point numbers.
///
/// # Arguments
/// * `binary_string`: String showing IEEE-754 binary representation of a number
///
/// # Return
/// * `num`: f64 version of the String
pub fn binary_to_f64(binary_string: &str) -> Result<f64> {
    // get sign and convert to bool as recompose expects
    let sign = &binary_string[0..1];
    let sign_bool = sign.parse::<i32>()? != 0;

    // convert exponent to int
    let exponent = &binary_string[1..12];
    let exponent_int = u16::from_str_radix(exponent, 2)?;

    // convert mantissa to int
    let mantissa = &binary_string[12..];
    let mantissa_int = u64::from_str_radix(mantissa, 2)?;

    // combine elements into f64 and return
    Ok(f64::recompose_raw(sign_bool, exponent_int, mantissa_int))
}

/// Takes `String` of form `{0,1}^64` and splits it into a sign, exponent, and mantissa
/// based on the IEEE-754 64-bit floating-point standard.
///
/// # Arguments
/// * `binary_string` - 64-bit binary string.
///
/// # Return
/// (sign, exponent, mantissa) - where each is a `String`.
pub fn split_ieee_into_components(binary_string: String) -> (String, String, String) {
    return (binary_string[0..1].to_string(), binary_string[1..12].to_string(), binary_string[12..].to_string());
}

/// Combines `String` versions of sign, exponent, and mantissa into
/// a single IEEE-754 64-bit floating-point representation.
///
/// # Arguments
/// * `sign` - Sign bit (length 1).
/// * `exponent` - Exponent bits (length 11).
/// * `mantissa` - Mantissa bits (length 52).
///
/// # Return
/// Concatenation of sign, exponent, and mantissa.
pub fn combine_components_into_ieee(
    (sign, exponent, mantissa): (String, String, String)
) -> String {
    vec![sign, exponent, mantissa].concat()
}

/// Samples a single element from a set according to provided weights.
///
/// # Arguments
/// * `candidate_set` - The set from which you want to sample.
/// * `weights` - Sampling weights for each element.
///
/// # Return
/// Element from the candidate set
#[cfg(feature="use-mpfr")]
pub fn sample_from_set<T>(
    candidate_set: &[T], weights: &[smartnoise_validator::Float],
    _enforce_constant_time: bool
) -> Result<T> where T: Clone {

    use rug::Float;

    // generate uniform random number on [0,1)
    let unif: rug::Float = Float::with_val(53, noise::sample_uniform_mpfr(0., 1.)?);

    // generate sum of weights
    let weights_rug: Vec<rug::Float> = weights.iter().map(|w| Float::with_val(53, w)).collect();
    let weights_sum: rug::Float = Float::with_val(53, Float::sum(weights_rug.iter()));

    // NOTE: use this instead of the two lines above if we switch to accepting rug::Float rather than f64 weights
    // let weights_sum: rug::Float = Float::with_val(53, Float::sum(weights.iter()));

    // convert weights to probabilities
    let probabilities: Vec<rug::Float> = weights_rug.iter().map(|w| w / weights_sum.clone()).collect();

    // generate cumulative probability distribution
    let mut cumulative_probability_vec: Vec<rug::Float> = Vec::with_capacity(weights.len() as usize);
    for i in 0..weights.len() {
        cumulative_probability_vec.push(Float::with_val(53, Float::sum(probabilities[0..(i + 1)].iter())));
    }

    // sample an element relative to its probability
    let mut return_index: usize = 0;
    for (i, cum_prob) in cumulative_probability_vec.into_iter().enumerate() {
        if unif <= cum_prob {
            return_index = i;
            break;
        }
    }
    Ok(candidate_set[return_index].clone())
}

#[cfg(not(feature="use-mpfr"))]
pub fn sample_from_set<T>(
    candidate_set: &[T], weights: &[smartnoise_validator::Float],
    enforce_constant_time: bool
) -> Result<T> where T: Clone {

    // generate uniform random number on [0,sum(weights))
    let sample: f64 = noise::sample_uniform(0., weights.iter().sum(), enforce_constant_time)?;

    // return once the cumulative weight reaches the uniform sample
    let mut cumulative = 0.;
    let mut return_index: usize = 0;
    loop {
        cumulative += weights[return_index];
        if cumulative >= sample { break }
        return_index += 1;
    }
    Ok(candidate_set[return_index].clone())
}

/// Accepts set and element weights and returns a subset of size k (without replacement).
///
/// Weights are (after being normalized) the probability of drawing each element on the first draw (they sum to 1)
/// Based on Algorithm A from Raimidis PS, Spirakis PG (2006). “Weighted random sampling with a reservoir.”
///
/// # Arguments
/// * `set` - Set of elements for which you would like to create a subset
/// * `weights` - Weight for each element in the set, corresponding to the probability it is drawn on the first draw.
/// * `k` - The size of the desired subset
///
/// # Return
/// subset of size k sampled according to weights
///
/// # Example
/// ```
/// use smartnoise_runtime::utilities::create_subset;
/// let set = vec![1, 2, 3, 4, 5, 6];
/// let weights = vec![1., 1., 1., 2., 2., 2.];
/// let k = 3;
/// let subset = create_subset(&set, &weights, k, false);
/// # subset.unwrap();
/// ```
#[cfg(feature="use-mpfr")]
pub fn create_subset<T>(
    set: &[T], weights: &[f64], k: usize,
    _enforce_constant_time: bool
) -> Result<Vec<T>> where T: Clone {
    if k > set.len() { return Err("k must be less than the set length".into()); }

    use rug::Float;
    use rug::ops::Pow;

    // generate sum of weights
    let weights_rug: Vec<rug::Float> = weights.iter().map(|w| Float::with_val(53, w)).collect();
    let weights_sum: rug::Float = Float::with_val(53, Float::sum(weights_rug.iter()));

    // convert weights to probabilities
    let probabilities: Vec<rug::Float> = weights_rug.iter().map(|w| w / weights_sum.clone()).collect();

    // generate keys and identify top k indices
    //

    // generate key/index tuples
    let mut key_vec = probabilities.into_iter()
        .take(set.len()).enumerate()
        .map(|(i, prob)| Ok((noise::sample_uniform_mpfr(0., 1.)?.pow(1. / prob), i)))
        .collect::<Result<Vec<(rug::Float, usize)>>>()?;

    // sort key/index tuples by key and identify top k indices
    let mut top_indices: Vec<usize> = Vec::with_capacity(k);
    key_vec.sort_by(|a, b|
        b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    top_indices.extend(key_vec.iter()
        .take(k).map(|v| v.1));

    // subsample based on top k indices
    let mut subset: Vec<T> = Vec::with_capacity(k);
    for value in top_indices.iter().map(|&index| set[index].clone()) {
        subset.push(value);
    }

    Ok(subset)
}

#[cfg(not(feature="use-mpfr"))]
pub fn create_subset<T>(
    set: &[T], weights: &[f64], k: usize,
    enforce_constant_time: bool
) -> Result<Vec<T>> where T: Clone {
    if k > set.len() { return Err("k must be less than the set length".into()); }

    // generate sum of weights
    let weights_sum: f64 = weights.iter().sum();

    // convert weights to probabilities
    let probabilities: Vec<f64> = weights.iter().map(|w| w / weights_sum).collect();

    // generate keys and identify top k indices
    //

    // generate key/index tuples
    let mut key_vec = (0..set.len())
        .map(|i| Ok((
            noise::sample_uniform(0., 1., enforce_constant_time)?
                .powf(1. / probabilities[i]),
            i
        )))
        .collect::<Result<Vec<(f64, usize)>>>()?;

    // sort key/index tuples by key and identify top k indices
    key_vec.sort_by(|a, b|
        b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // subsample based on top k indices
    Ok(key_vec.iter().take(k).map(|v| set[v.1].clone()).collect())
}


/// Finds the closest number to x that is a multiple of Lambda.
///
/// # Arguments
/// * `x` - Number to be rounded to closest multiple of Lambda.
/// * `m` - Integer such that Lambda = 2^m.
///
/// # Returns
/// Closest multiple of Lambda to x.
pub fn get_closest_multiple_of_lambda(x: f64, m: i16) -> Result<f64> {
    let (sign, mut exponent, mantissa) = x.decompose();
    exponent -= m;

    let (sign, mut exponent, mantissa) = match exponent {
        // original components already represent an integer (decimal shifted >= 52 places on mantissa)
        exponent if exponent >= 52 => (sign, exponent, mantissa),
        // round int to +- 1
        exponent if exponent == -1 => (sign, 0, 0),
        // round int to 0, and keep it zero after adding m
        exponent if exponent < -1 => (sign, -1023 - m, 0),
        // round to int when decimal is within range of mantissa
        _ => {
            // get elements of mantissa that represent integers (after decimal is shifted by "exponent" places)
            //     shift 1 "exponent" places to the left (no overflow because exponent < 64)
            //     subtract one to set "exponent" bits to one
            //     shift the mask to the left for a 52-bit mask that keeps the top #"exponent" bits
            let integer_mask: u64 = ((1u64 << exponent) - 1) << (52 - exponent);
            let integer_mantissa: u64 = mantissa & integer_mask;

            // check if digit after exponent point is set
            if mantissa & (1u64 << (52 - (exponent + 1))) == 0u64 {
                (sign, exponent, integer_mantissa)
            } else {
                // if integer part of mantissa is all 1s, rounding needs to be reflected in the exponent instead
                if integer_mantissa == integer_mask {
                    (sign, exponent + 1, 0)
                } else {
                    (sign, exponent, integer_mantissa + (1u64 << (52 - exponent)))
                }
            }
        }
    };

    exponent += m;
    Ok(f64::recompose(sign, exponent, mantissa))
}

#[cfg(test)]
mod test_get_closest_multiple_of_lambda {
    use smartnoise_validator::hashmap;
    use crate::utilities::get_closest_multiple_of_lambda;

    #[test]
    fn test_get_closest_multiple_of_lambda_range() {
        (0..100).for_each(|i| {
            let x = 1. - 0.01 * (i as f64);
            println!("{}: {}", x, get_closest_multiple_of_lambda(x, -1).unwrap())
        });
    }

    #[test]
    fn test_get_closest_multiple_of_lambda() {
        let input = vec![-30.01, -2.51, -1.01, -0.76, -0.51, -0.26, 0.0, 0.26, 0.51, 0.76, 1.01, 2.51, 30.01];

        hashmap![
            -2 => vec![-30., -2.5, -1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0, 2.5, 30.0],
            -1 => vec![-30., -2.5, -1.0, -1.0, -0.5, -0.5, 0.0, 0.5, 0.5, 1.0, 1.0, 2.5, 30.0],
            0 => vec![-30., -3.0, -1.0, -1.0, -1.0, -0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 3.0, 30.0],
            1 => vec![-30., -2.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 30.0],
            2 => vec![-32., -4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 32.0]
        ].into_iter().for_each(|(m, outputs)| {
            input.iter().copied().zip(outputs.into_iter())
                .for_each(|(input, expected)| {
                    let actual = get_closest_multiple_of_lambda(input, m).unwrap();
                    println!("m: {:?}, input: {:?}, actual: {:?}, expected: {:?}",
                             m, input, actual, expected);
                    assert_eq!(actual, expected)
                })
        });
    }
}
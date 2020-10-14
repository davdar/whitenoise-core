use ndarray::{arr0};

use smartnoise_validator::{Float, Integer, proto};
use smartnoise_validator::base::{Array, ReleaseNode, Value};
use smartnoise_validator::errors::*;
use smartnoise_validator::utilities::{array::broadcast_ndarray, privacy::{get_delta, get_epsilon, spread_privacy_usage}, take_argument};

use crate::components::Evaluable;
use crate::NodeArguments;
use crate::utilities;
use crate::utilities::{get_num_columns, to_nd};
use crate::utilities::mechanisms::exponential_mechanism;

impl Evaluable for proto::LaplaceMechanism {
    fn evaluate(
        &self,
        privacy_definition: &Option<proto::PrivacyDefinition>,
        mut arguments: NodeArguments
    ) -> Result<ReleaseNode> {

        let enforce_constant_time = privacy_definition.as_ref()
            .map(|v| v.protect_elapsed_time).unwrap_or(false);

        let data = take_argument(&mut arguments, "data")?.array()?;
        let num_columns = data.num_columns()?;
        let mut data = match data {
            Array::Float(data) => data,
            Array::Int(data) => data.mapv(|v| v as Float),
            _ => return Err("data must be numeric".into())
        };

        let sensitivity = take_argument(&mut arguments, "sensitivity")?.array()?.float()?;

        let usages = spread_privacy_usage(&self.privacy_usage, num_columns)?;
        let epsilon = usages.iter().map(get_epsilon).collect::<Result<Vec<f64>>>()?;

        data.gencolumns_mut().into_iter()
            .zip(sensitivity.gencolumns().into_iter().zip(epsilon.into_iter()))
            .try_for_each(|(mut data_column, (sensitivity, epsilon))| data_column.iter_mut()
                .zip(sensitivity.iter())
                .try_for_each(|(v, sens)|

                    utilities::mechanisms::laplace_mechanism(
                        epsilon, *sens as f64,
                        enforce_constant_time,
                    ).map(|noise| *v += noise as Float)))?;

        Ok(ReleaseNode {
            value: data.into(),
            privacy_usages: Some(usages),
            public: true,
        })
    }
}

impl Evaluable for proto::GaussianMechanism {
    fn evaluate(
        &self,
        privacy_definition: &Option<proto::PrivacyDefinition>,
        mut arguments: NodeArguments
    ) -> Result<ReleaseNode> {

        let enforce_constant_time = privacy_definition.as_ref()
            .map(|v| v.protect_elapsed_time).unwrap_or(false);

        let data = take_argument(&mut arguments, "data")?.array()?;
        let num_columns = data.num_columns()?;
        let mut data = match data {
            Array::Float(data) => data,
            Array::Int(data) => data.mapv(|v| v as Float),
            _ => return Err("data must be numeric".into())
        };

        let sensitivity = take_argument(&mut arguments, "sensitivity")?.array()?.float()?;

        let usages = spread_privacy_usage(&self.privacy_usage, num_columns)?;

        let epsilon = usages.iter().map(get_epsilon).collect::<Result<Vec<f64>>>()?;
        let delta = usages.iter().map(get_delta).collect::<Result<Vec<f64>>>()?;

        data.gencolumns_mut().into_iter()
            .zip(sensitivity.gencolumns().into_iter())
            .zip(epsilon.into_iter().zip(delta.into_iter()))
            .try_for_each(|((mut data_column, sensitivity), (epsilon, delta))| data_column.iter_mut()
                .zip(sensitivity.iter())
                .try_for_each(|(v, sens)|

                    utilities::mechanisms::gaussian_mechanism(
                        epsilon, delta, *sens as f64, self.analytic,
                        enforce_constant_time,
                    ).map(|noise| *v += noise as Float)))?;

        Ok(ReleaseNode {
            value: data.into(),
            privacy_usages: Some(usages),
            public: true,
        })
    }
}

impl Evaluable for proto::SimpleGeometricMechanism {
    fn evaluate(&self, privacy_definition: &Option<proto::PrivacyDefinition>, mut arguments: NodeArguments) -> Result<ReleaseNode> {

        let enforce_constant_time = privacy_definition.as_ref()
            .map(|v| v.protect_elapsed_time).unwrap_or(false);

        let data = take_argument(&mut arguments, "data")?.array()?;
        let num_columns = data.num_columns()?;
        let mut data = data.int()?.to_owned();

        let sensitivity = take_argument(&mut arguments, "sensitivity")?.array()?.float()?;

        let usages = spread_privacy_usage(&self.privacy_usage, num_columns)?;
        let epsilon = usages.iter().map(get_epsilon).collect::<Result<Vec<f64>>>()?;

        let lower = broadcast_ndarray(
            take_argument(&mut arguments, "lower")?.array()?.int()?, data.shape())?;

        let upper = broadcast_ndarray(
            take_argument(&mut arguments, "upper")?.array()?.int()?, data.shape())?;

        data.gencolumns_mut().into_iter()
            .zip(sensitivity.gencolumns().into_iter().zip(epsilon.into_iter()))
            .zip(lower.gencolumns().into_iter().zip(upper.gencolumns().into_iter()))
            .try_for_each(|((mut data_column, (sensitivity, epsilon)), (lower, upper))| data_column.iter_mut()
                .zip(sensitivity.iter())
                .zip(lower.iter().zip(upper.iter()))
                .try_for_each(|((v, sens), (c_min, c_max))|

                    utilities::mechanisms::simple_geometric_mechanism(
                        epsilon, *sens as f64,
                        *c_min as i64, *c_max as i64,
                        enforce_constant_time,
                    ).map(|noise| *v += noise as Integer)))?;

        Ok(ReleaseNode {
            value: data.into(),
            privacy_usages: Some(usages),
            public: true,
        })
    }
}

impl Evaluable for proto::ExponentialMechanism {
    fn evaluate(
        &self, privacy_definition: &Option<proto::PrivacyDefinition>, mut arguments: NodeArguments,
    ) -> Result<ReleaseNode> {
        let enforce_constant_time = privacy_definition.as_ref()
            .map(|v| v.protect_elapsed_time).unwrap_or(false);

        let candidates = take_argument(&mut arguments, "candidates")?.array()?;

        let sensitivity = take_argument(&mut arguments, "sensitivity")?.array()?.float()?
            .iter().cloned().collect::<Vec<Float>>();

        let usages = spread_privacy_usage(&self.privacy_usage, sensitivity.len())?;
        let epsilon = usages.iter().map(get_epsilon).collect::<Result<Vec<f64>>>()?;

        let utilities = take_argument(&mut arguments, "utilities")?.array()?.float()?;

        macro_rules! apply_exponential {
            ($candidates:ident) => {
                {
                    let mut release_vec = $candidates.gencolumns().into_iter()
                        .zip(utilities.gencolumns().into_iter())
                        .zip(sensitivity.iter().zip(epsilon.iter()))
                        .map(|((cands, utils), (sens, eps))| exponential_mechanism(
                            *eps, *sens as f64,
                            &cands.to_vec(),
                            utils.into_iter().map(|v| *v as f64).collect(),
                            enforce_constant_time))
                        .collect::<Result<Vec<_>>>()?;

                    Value::from(arr0(release_vec.remove(0)).into_dyn())
                }
            }
        }

        Ok(ReleaseNode {
            value: match candidates {
                Array::Float(candidates) => apply_exponential!(candidates),
                Array::Int(candidates) => apply_exponential!(candidates),
                Array::Str(candidates) => apply_exponential!(candidates),
                Array::Bool(candidates) => apply_exponential!(candidates)
            },
            privacy_usages: Some(usages),
            public: true,
        })
    }
}

impl Evaluable for proto::SnappingMechanism {
    fn evaluate(&self, privacy_definition: &Option<proto::PrivacyDefinition>, mut arguments: NodeArguments) -> Result<ReleaseNode> {
        let mut data = match take_argument(&mut arguments, "data")?.array()? {
            Array::Float(data) => data.clone(),
            Array::Int(data) => data.mapv(|v| v as f64),
            _ => return Err("data must be numeric".into())
        };

        let enforce_constant_time = privacy_definition.as_ref()
            .map(|v| v.protect_elapsed_time).unwrap_or(false);

        let sensitivity = take_argument(&mut arguments, "sensitivity")?
            .array()?.float()?;

        let usages = spread_privacy_usage(
            &self.privacy_usage, sensitivity.len())?;

        let epsilon = ndarray::Array::from_shape_vec(
            data.shape(), usages.iter().map(get_epsilon).collect::<Result<Vec<f64>>>()?)?;

        let num_columns = get_num_columns(&data)? as usize;

        let lower = to_nd(match take_argument(&mut arguments, "lower")?.array()? {
            Array::Float(l) => l,
            Array::Int(l) => l.mapv(|v| v as Float),
            _ => return Err("lower: must be numeric".into())
        }, 1)?.into_dimensionality::<ndarray::Ix1>()?.to_vec();

        if num_columns != lower.len() {
            return Err("lower must share the same number of columns as data".into())
        }

        let upper = to_nd(match take_argument(&mut arguments, "upper")?.array()? {
            Array::Float(u) => u,
            Array::Int(u) => u.mapv(|v| v as Float),
            _ => return Err("upper: must be numeric".into())
        }, 1)?.into_dimensionality::<ndarray::Ix1>()?.to_vec();

        if num_columns != upper.len() {
            return Err("upper must share the same number of columns as data".into())
        }

        let binding_probability = match take_argument(&mut arguments, "binding_probability") {
            Ok(prob) => Some(prob.array()?.first_float()?),
            _ => None
        };

        data.gencolumns_mut().into_iter()
            .zip(sensitivity.gencolumns().into_iter().zip(epsilon.gencolumns().into_iter()))
            .zip(lower.into_iter().zip(upper.into_iter()))
            .try_for_each(|((mut data_column, (sensitivity, epsilon)), (lower, upper))| data_column.iter_mut()
                .zip(sensitivity.into_iter().zip(epsilon.into_iter()))
                .try_for_each(|(v, (sens, eps))|

                    utilities::mechanisms::snapping_mechanism(
                        *v, *eps, *sens as f64,
                        lower, upper, binding_probability,
                        enforce_constant_time
                    ).map(|privatized| *v = privatized as Float)))?;

        Ok(ReleaseNode {
            value: data.into(),
            privacy_usages: Some(usages),
            public: true
        })
    }
}
use crate::errors::*;


use crate::components::{Sensitivity, Accuracy, Mechanism};
use crate::{proto, base, Warnable};

use crate::components::{Component, Expandable};
use crate::base::{Value, SensitivitySpace, ValueProperties, DataType, NodeProperties, IndexKey};
use crate::utilities::{prepend, expand_mechanism};
use crate::utilities::privacy::{spread_privacy_usage, get_epsilon, privacy_usage_check};
use itertools::Itertools;
use indexmap::map::IndexMap;


impl Component for proto::LaplaceMechanism {
    fn propagate_property(
        &self,
        privacy_definition: &Option<proto::PrivacyDefinition>,
        _public_arguments: &IndexMap<base::IndexKey, Value>,
        properties: &base::NodeProperties,
        _node_id: u32
    ) -> Result<Warnable<ValueProperties>> {

        let privacy_definition = privacy_definition.as_ref()
            .ok_or_else(|| "privacy_definition must be defined")?;

        if privacy_definition.protect_floating_point {
            return Err("Floating-point protections are enabled. The laplace mechanism is susceptible to floating-point attacks.".into())
        }

        if privacy_definition.group_size == 0 {
            return Err("group size must be greater than zero".into())
        }

        let mut data_property = properties.get::<IndexKey>(&"data".into())
            .ok_or("data: missing")?.array()
            .map_err(prepend("data:"))?.clone();

        if data_property.data_type != DataType::F64 {
            return Err("data: atomic type must be float".into())
        }

        let aggregator = data_property.aggregator.clone()
            .ok_or_else(|| Error::from("aggregator: missing"))?;

        // sensitivity must be computable
        let mut sensitivity_values = aggregator.component.compute_sensitivity(
            privacy_definition,
            &aggregator.properties,
            &SensitivitySpace::KNorm(1))?;

        if aggregator.lipschitz_constant.iter().any(|v| v != &1.) {
            let mut sensitivity = sensitivity_values.array()?.f64()?.clone();
            sensitivity.gencolumns_mut().into_iter()
                .zip(aggregator.lipschitz_constant.iter())
                .for_each(|(mut sens, cons)| sens.iter_mut()
                    .for_each(|v| *v *= cons));
            sensitivity_values = sensitivity.into();
        }

        // make sure sensitivities are an f64 array
        sensitivity_values.array()?.f64()?;

        let privacy_usage = self.privacy_usage.iter().cloned().map(Ok)
            .fold1(|l, r| l? + r?).ok_or_else(|| "privacy_usage: must be defined")??;

        let warnings = privacy_usage_check(
            &privacy_usage,
            data_property.num_records,
            privacy_definition.strict_parameter_checks)?;

        data_property.releasable = true;
        data_property.aggregator = None;

        Ok(Warnable(data_property.into(), warnings))
    }
}


impl Expandable for proto::LaplaceMechanism {
    fn expand_component(
        &self,
        privacy_definition: &Option<proto::PrivacyDefinition>,
        component: &proto::Component,
        properties: &base::NodeProperties,
        component_id: &u32,
        maximum_id: &u32,
    ) -> Result<proto::ComponentExpansion> {
        expand_mechanism(
            &SensitivitySpace::KNorm(1),
            privacy_definition,
            self.privacy_usage.as_ref(),
            component,
            properties,
            component_id,
            maximum_id
        )
    }
}

impl Mechanism for proto::LaplaceMechanism {
    fn get_privacy_usage(
        &self,
        privacy_definition: &proto::PrivacyDefinition,
        release_usage: Option<&Vec<proto::PrivacyUsage>>,
        properties: &NodeProperties
    ) -> Result<Option<Vec<proto::PrivacyUsage>>> {
        let data_property = properties.get::<IndexKey>(&"data".into())
            .ok_or("data: missing")?.array()
            .map_err(prepend("data:"))?;
        Ok(Some(match release_usage {
            Some(release_usage) => release_usage.iter()
                .zip(data_property.c_stability.iter())
                .map(|(usage, c_stab)|
                    usage.effective_to_actual(1., *c_stab, privacy_definition.group_size))
                .collect::<Result<Vec<proto::PrivacyUsage>>>()?,
            None => self.privacy_usage.clone()
        }))
    }
}


impl Accuracy for proto::LaplaceMechanism {
    fn accuracy_to_privacy_usage(
        &self,
        privacy_definition: &proto::PrivacyDefinition,
        properties: &base::NodeProperties,
        accuracies: &proto::Accuracies,
    ) -> Result<Option<Vec<proto::PrivacyUsage>>> {
        let data_property = properties.get::<IndexKey>(&"data".into())
            .ok_or("data: missing")?.array()
            .map_err(prepend("data:"))?.clone();

        let aggregator = data_property.aggregator.clone()
            .ok_or_else(|| Error::from("aggregator: missing"))?;

        let sensitivity_values = aggregator.component.compute_sensitivity(
            &privacy_definition,
            &aggregator.properties,
            &SensitivitySpace::KNorm(1))?;

        // sensitivity must be computable
        let sensitivities = sensitivity_values.array()?.f64()?;

        Ok(Some(sensitivities.into_iter().zip(accuracies.values.iter())
            .map(|(sensitivity, accuracy)| proto::PrivacyUsage {
                distance: Some(proto::privacy_usage::Distance::Approximate(proto::privacy_usage::DistanceApproximate {
                    epsilon: (1. / accuracy.alpha).ln() * (sensitivity / accuracy.value),
                    delta: 0.,
                }))
            })
            .collect()))
    }

    fn privacy_usage_to_accuracy(
        &self,
        privacy_definition: &proto::PrivacyDefinition,
        properties: &base::NodeProperties,
        alpha: &f64
    ) -> Result<Option<Vec<proto::Accuracy>>> {
        let data_property = properties.get::<IndexKey>(&"data".into())
            .ok_or("data: missing")?.array()
            .map_err(prepend("data:"))?.clone();

        let aggregator = data_property.aggregator.clone()
            .ok_or_else(|| Error::from("aggregator: missing"))?;

        let sensitivity_values = aggregator.component.compute_sensitivity(
            &privacy_definition,
            &aggregator.properties,
            &SensitivitySpace::KNorm(1))?;

        // sensitivity must be computable
        let sensitivities = sensitivity_values.array()?.f64()?;

        let usages = spread_privacy_usage(&self.privacy_usage, sensitivities.len())?;
        let epsilons = usages.iter().map(get_epsilon).collect::<Result<Vec<f64>>>()?;

        Ok(Some(sensitivities.into_iter().zip(epsilons.into_iter())
            .map(|(sensitivity, epsilon)| proto::Accuracy {
                value: (1. / *alpha).ln() * (sensitivity / epsilon),
                alpha: *alpha,
            })
            .collect()))
    }
}
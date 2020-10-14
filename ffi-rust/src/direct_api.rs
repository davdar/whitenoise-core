use smartnoise_runtime::utilities::mechanisms;

#[no_mangle]
pub extern "C" fn laplace_mechanism(
    value: f64, epsilon: f64, sensitivity: f64, enforce_constant_time: bool
) -> f64 {
    value + mechanisms::laplace_mechanism(
        epsilon, sensitivity, enforce_constant_time).unwrap()
}

#[no_mangle]
pub extern "C" fn gaussian_mechanism(
    value: f64, epsilon: f64, delta: f64, sensitivity: f64,
    analytic: bool,
    enforce_constant_time: bool,
) -> f64 {
    value + mechanisms::gaussian_mechanism(
        epsilon, delta, sensitivity, analytic, enforce_constant_time).unwrap()
}

#[no_mangle]
pub extern "C" fn simple_geometric_mechanism(
    value: i64,
    epsilon: f64, sensitivity: f64,
    min: i64, max: i64,
    enforce_constant_time: bool
) -> i64 {
    value + mechanisms::simple_geometric_mechanism(
        epsilon, sensitivity, min, max, enforce_constant_time).unwrap()
}


#[no_mangle]
pub extern "C" fn snapping_mechanism(
    value: f64, epsilon: f64, sensitivity: f64,
    min: f64, max: f64,
    enforce_constant_time: bool
) -> f64 {
    mechanisms::snapping_mechanism(
        value, epsilon, sensitivity,
        min, max, None,
        enforce_constant_time).unwrap()
}

#[no_mangle]
pub extern "C" fn snapping_mechanism_binding(
    value: f64, epsilon: f64, sensitivity: f64,
    min: f64, max: f64, binding_probability: f64,
    enforce_constant_time: bool
) -> f64 {
    mechanisms::snapping_mechanism(
        value, epsilon, sensitivity,
        min, max, Some(binding_probability),
        enforce_constant_time).unwrap()
}
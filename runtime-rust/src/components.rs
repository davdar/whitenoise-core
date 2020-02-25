extern crate yarrow_validator;
use yarrow_validator::proto;
use crate::utilities;
use crate::base::*;

use ndarray::prelude::*;
use std::collections::HashMap;

extern crate csv;
extern crate num;

use std::str::FromStr;
use yarrow_validator::utilities::buffer::{
    NodeArguments, get_f64, get_array_f64, get_array_bool, get_bool, get_i64};
use ndarray::stack;
use yarrow_validator::utilities::serial::{Value, parse_value, ArrayND, Vector2DJagged};

macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

pub fn component_literal(x: &proto::Literal) -> Result<Value, String> {
    parse_value(&x.to_owned().value.unwrap())
}

pub fn component_materialize(
    materialize: &proto::Materialize,
    dataset: &proto::Dataset
) -> Result<Value, String> {
    let table = dataset.tables.get(&materialize.dataset_id).unwrap();
    match table.value.as_ref().unwrap() {
        proto::table::Value::Literal(value) => parse_value(value),
        proto::table::Value::FilePath(path) => {
            let mut response = HashMap::<String, Vec<String>>::new();
            csv::Reader::from_path(path).unwrap().deserialize()
                .for_each(|result| {
                    // parse each record into the yarrow internal format
                    let record: HashMap<String, String> = result.unwrap();
                    record.iter().for_each(|(k, v)| response
                        .entry(k.to_owned()).or_insert_with(Vec::new)
                        .push(v.clone()));
                });
            Ok(Value::HashmapString(response.iter()
                .map(|(k, v): (&String, &Vec<String>)| (
                    k.clone(), Value::ArrayND(ArrayND::Str(Array::from(v.to_owned()).into_dyn()))
                ))
                .collect::<HashMap<String, Value>>()))
        },
        _ => Err("the selected table reference format is not implemented".to_string())
    }
}

pub fn component_index(index: &proto::Index, arguments: &NodeArguments) -> Result<Value, String> {
    let data = arguments.get("data").unwrap();
    let columns = arguments.get("columns").unwrap();

    match data {
        Value::HashmapString(dataframe) => match columns {
            Value::ArrayND(array) => match array {
                ArrayND::Str(column_names) => match column_names.ndim() {
                    0 => Ok(dataframe.get(column_names.first().unwrap()).unwrap().to_owned()),
//                1 => match column_names.into_dimensionality::<Ix1>() {
//                    Ok(column_names) =>
//                        Value::Str(stack(Axis(0), column_names.to_vec().iter()
//                            .map(|column_name| match dataframe.get(column_names.first().unwrap()).unwrap() {
//                                Value::Str(array) => array,
//                                _ => panic!("selected data frame columns are not of a homogenous type".to_string())
//                            }).collect()).unwrap())
//                            .collect::<Vec<ArrayD<str>>>(),
//                    _ => Err("column names must be at most 1-dimensional".to_owned()),
//                },
                    _ => Err("column names must be at most 1-dimensional".to_owned())
                },
                _ => Err("column names must be strings".to_string())
            },
            _ => Err("column names must an array".to_string())
        },
        _ => Err("indexing is only implemented for hashmaps".to_string())
    }
}

pub fn component_datasource(
    datasource: &proto::DataSource, dataset: &proto::Dataset, arguments: &NodeArguments
) -> Result<Value, String> {
//    println!("datasource");

    let table = dataset.tables.get(&datasource.dataset_id).unwrap();
    Ok(match table.value.as_ref().unwrap() {
        proto::table::Value::FilePath(path) => {

            fn get_column<T>(path: &String, column: &String) -> Vec<T>
                where T: FromStr, <T as std::str::FromStr>::Err: std::fmt::Debug {
                let mut rdr = csv::Reader::from_path(path).unwrap();
                rdr.deserialize().map(|result| {
                    let record: HashMap<String, String> = result.unwrap();
//                    println!("{:?}", record);
                    record[column].parse::<T>().unwrap()
                }).collect()
            }

            match arguments.get("datatype").unwrap() {
                Value::ArrayND(array) => match array {
                    ArrayND::Str(x) => Ok(match x.first().unwrap().as_ref() {
//                    "BYTES" =>
//                        Ok(Value::Bytes(Array1::from(get_column::<u8>(&path, &datasource.column_id)).into_dyn())),
                        "BOOL" =>
                            Ok(Value::ArrayND(ArrayND::Bool(Array1::from(get_column::<bool>(&path, &datasource.column_id)).into_dyn()))),
                        "I64" =>
                            Ok(Value::ArrayND(ArrayND::I64(Array1::from(get_column::<i64>(&path, &datasource.column_id)).into_dyn()))),
                        "F64" =>
                            Ok(Value::ArrayND(ArrayND::F64(Array1::from(get_column::<f64>(&path, &datasource.column_id)).into_dyn()))),
                        "STRING" =>
                            Ok(Value::ArrayND(ArrayND::Str(Array1::from(get_column::<String>(&path, &datasource.column_id)).into_dyn()))),
                        _ => Err("Datatype is not recognized.".to_string())
                    }.unwrap()),
                    _ => Err("Datatype must be a string.".to_string())
                }
                _ => Err("Datatype must be contained in an array.".to_string())
            }
        },
        proto::table::Value::Literal(value) => parse_value(&value),
        _ => Err("Only file paths are supported".to_string())
    }.unwrap())
}

pub fn component_add(
    _x: &proto::Add, arguments: &NodeArguments
) -> Result<Value, String> {
//    println!("add");
    match (arguments.get("left").unwrap(), arguments.get("right").unwrap()) {
        (Value::ArrayND(left), Value::ArrayND(right)) => match (left, right) {
            (ArrayND::F64(x), ArrayND::F64(y)) =>
                Ok(Value::ArrayND(ArrayND::F64(x + y))),
            (ArrayND::I64(x), ArrayND::I64(y)) =>
                Ok(Value::ArrayND(ArrayND::I64(x + y))),
            _ => Err("Add: Either the argument types are mismatched or non-numeric.".to_string())
        },
        _ => Err("Add: Both arguments must be arrays.".to_string())
    }
}


pub fn component_subtract(
    _x: &proto::Subtract, arguments: &NodeArguments
) -> Result<Value, String> {

    match (arguments.get("left").unwrap(), arguments.get("right").unwrap()) {
        (Value::ArrayND(left), Value::ArrayND(right)) => match (left, right) {
            (ArrayND::F64(x), ArrayND::F64(y)) =>
                Ok(Value::ArrayND(ArrayND::F64(x - y))),
            (ArrayND::I64(x), ArrayND::I64(y)) =>
                Ok(Value::ArrayND(ArrayND::I64(x - y))),
            _ => Err("Subtract: Either the argument types are mismatched or non-numeric.".to_string())
        },
        _ => Err("Subtract: Both arguments must be arrays.".to_string())
    }
}

pub fn component_divide(
    _x: &proto::Divide, arguments: &NodeArguments
) -> Result<Value, String> {

    match (arguments.get("left").unwrap(), arguments.get("right").unwrap()) {
        (Value::ArrayND(left), Value::ArrayND(right)) => match (left, right) {
            (ArrayND::F64(x), ArrayND::F64(y)) =>
                Ok(Value::ArrayND(ArrayND::F64(x / y))),
            (ArrayND::I64(x), ArrayND::I64(y)) =>
                Ok(Value::ArrayND(ArrayND::I64(x / y))),
            _ => Err("Divide: Either the argument types are mismatched or non-numeric.".to_string())
        },
        _ => Err("Divide: Both arguments must be arrays.".to_string())
    }
}

pub fn component_multiply(
    _x: &proto::Multiply, arguments: &NodeArguments
) -> Result<Value, String> {
    match (arguments.get("left").unwrap(), arguments.get("right").unwrap()) {
        (Value::ArrayND(left), Value::ArrayND(right)) => match (left, right) {
            (ArrayND::F64(x), ArrayND::F64(y)) =>
                Ok(Value::ArrayND(ArrayND::F64(x * y))),
            (ArrayND::I64(x), ArrayND::I64(y)) =>
                Ok(Value::ArrayND(ArrayND::I64(x * y))),
            _ => Err("Multiply: Either the argument types are mismatched or non-numeric.".to_string())
        },
        _ => Err("Multiply: Both arguments must be arrays.".to_string())
    }
}

pub fn component_power(
    _x: &proto::Power, arguments: &NodeArguments
) -> Result<Value, String> {
    let power: f64 = get_f64(&arguments, "right");
    let data = get_array_f64(&arguments, "left");
    Ok(Value::ArrayND(ArrayND::F64(data.mapv(|x| x.powf(power)))))
}

pub fn component_negate(
    _x: &proto::Negate, arguments: &NodeArguments
) -> Result<Value, String> {

    match arguments.get("data").unwrap() {
        Value::ArrayND(data) => match data {
            ArrayND::F64(x) =>
                Ok(Value::ArrayND(ArrayND::F64(-x))),
            ArrayND::I64(x) =>
                Ok(Value::ArrayND(ArrayND::I64(-x))),
            _ => Err("Negate: Argument must be numeric.".to_string())
        },
        _ => Err("Negate: Argument must be an array.".to_string())
    }
}

pub fn component_bin(
    _X: &proto::Bin, arguments: &NodeArguments
) -> Result<Value, String> {

    let data: ArrayD<f64> = get_array_f64(&arguments, "data");
    let edges: ArrayD<f64> = get_array_f64(&arguments, "edges");
    let inclusive_left: ArrayD<bool> = get_array_bool(&arguments, "inclusive_left");
    Ok(Value::ArrayND(ArrayND::Str(utilities::transformations::bin(&data, &edges, &inclusive_left))))
}

pub fn component_row_wise_min(
    _x: &proto::RowMin, arguments: &NodeArguments
) -> Result<Value, String> {
    match (arguments.get("left").unwrap(), arguments.get("right").unwrap()) {
        (Value::ArrayND(left), Value::ArrayND(right)) => match (left, right) {
            (ArrayND::F64(x), ArrayND::F64(y)) =>
                Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::broadcast_map(
                    &x, &y, &|l: &f64, r: &f64| l.min(*r))?))),
            (ArrayND::I64(x), ArrayND::I64(y)) =>
                Ok(Value::ArrayND(ArrayND::I64(utilities::transformations::broadcast_map(
                    &x, &y, &|l: &i64, r: &i64| *std::cmp::max(l, r))?))),
            _ => Err("Min: Either the argument types are mismatched or non-numeric.".to_string())
        },
        _ => Err("Min: Both arguments must be arrays.".to_string())
    }
}

pub fn component_row_wise_max(
    _x: &proto::RowMax, arguments: &NodeArguments,
) -> Result<Value, String> {
    match (arguments.get("left").unwrap(), arguments.get("right").unwrap()) {
        (Value::ArrayND(left), Value::ArrayND(right)) => match (left, right) {
            (ArrayND::F64(x), ArrayND::F64(y)) =>
                Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::broadcast_map(
                    &x, &y, &|l: &f64, r: &f64| l.max(*r))?))),
            (ArrayND::I64(x), ArrayND::I64(y)) =>
                Ok(Value::ArrayND(ArrayND::I64(utilities::transformations::broadcast_map(
                    &x, &y, &|l: &i64, r: &i64| *std::cmp::max(l, r))?))),
            _ => Err("Max: Either the argument types are mismatched or non-numeric.".to_string())
        },
        _ => Err("Max: Both arguments must be arrays.".to_string())
    }
}

pub fn component_clamp(_x: &proto::Clamp, arguments: &NodeArguments,) -> Result<Value, String> {
    if arguments.contains_key("categories") {
        match (arguments.get("data").unwrap(), arguments.get("categories").unwrap(), arguments.get("null").unwrap()) {
            (Value::ArrayND(data), Value::Vector2DJagged(categories), Value::ArrayND(null)) => match (data, categories, null) {
                (ArrayND::Bool(data), Vector2DJagged::Bool(categories), ArrayND::Bool(null)) =>
                    {
                        let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<bool>>>();
                        return Ok(Value::ArrayND(ArrayND::Bool(utilities::transformations::clamp_categorical(&data, &categories, &null))));
                    },
                (ArrayND::F64(data), Vector2DJagged::F64(categories), ArrayND::F64(null)) =>
                    {
                        let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
                        return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::clamp_categorical(&data, &categories, &null))));
                    },
                (ArrayND::I64(data), Vector2DJagged::I64(categories), ArrayND::I64(null)) =>
                    {
                        let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<i64>>>();
                        return Ok(Value::ArrayND(ArrayND::I64(utilities::transformations::clamp_categorical(&data, &categories, &null))));
                    },
                (ArrayND::Str(data), Vector2DJagged::Str(categories), ArrayND::Str(null)) =>
                    {
                        let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<String>>>();
                        return Ok(Value::ArrayND(ArrayND::Str(utilities::transformations::clamp_categorical(&data, &categories, &null))));
                    },
                _ => return Err("types of data, categories, and null must be consistent".to_string())
            },
            _ => return Err("data must be ArrayND, categories must be Vector2DJagged, and null must be ArrayND".to_string())
        }
    } else {
        match (arguments.get("data").unwrap(), arguments.get("min").unwrap(), arguments.get("max").unwrap()) {
            (Value::ArrayND(data), Value::ArrayND(min), Value::ArrayND(max)) => match(data, min, max) {
                (ArrayND::F64(data), ArrayND::F64(min), ArrayND::F64(max)) =>
                    return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::clamp_numeric(&data, &min, &max)))),
                (ArrayND::I64(data), ArrayND::I64(min), ArrayND::I64(max)) =>
                    return Ok(Value::ArrayND(ArrayND::I64(utilities::transformations::clamp_numeric(&data, &min, &max)))),
                _ =>
                    return Err("data, min, and max must all have type f64 or i64".to_string())
            },
            _ => return Err("data, min, and max must all be ArrayND".to_string())
        }
    }
}

// TODO: still working on this
pub fn component_impute(_x: &proto::Impute, arguments: &NodeArguments,) -> Result<Value, String> {
    let Uniform: String = "Uniform".to_string(); // Distributions
    let Gaussian: String = "Gaussian".to_string();
    let Float: String = "Float".to_string(); // Data Types
    let Int: String = "Int".to_string();

    if arguments.contains_key("categories") {
        match (arguments.get("data").unwrap(), arguments.get("categories").unwrap(), arguments.get("probabilities").unwrap(), arguments.get("null").unwrap()) {
            (Value::ArrayND(data), Value::Vector2DJagged(categories), Value::Vector2DJagged(probabilities), Value::ArrayND(null)) => match (data, categories, probabilities, null) {
                (ArrayND::Bool(data), Vector2DJagged::Bool(categories), Vector2DJagged::F64(probabilities), ArrayND::Bool(null)) =>
                    {
                        let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<bool>>>();
                        let probabilities = probabilities.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
                        return Ok(Value::ArrayND(ArrayND::Bool(utilities::transformations::impute_categorical(&data, &categories, &probabilities, &null))));
                    },
                (ArrayND::F64(data), Vector2DJagged::F64(categories), Vector2DJagged::F64(probabilities), ArrayND::F64(null)) =>
                    {
                        let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
                        let probabilities = probabilities.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
                        return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::impute_categorical(&data, &categories, &probabilities, &null))));
                    },
                (ArrayND::I64(data), Vector2DJagged::I64(categories), Vector2DJagged::F64(probabilities), ArrayND::I64(null)) =>
                    {
                        let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<i64>>>();
                        let probabilities = probabilities.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
                        return Ok(Value::ArrayND(ArrayND::I64(utilities::transformations::impute_categorical(&data, &categories, &probabilities, &null))));
                    },
                (ArrayND::Str(data), Vector2DJagged::Str(categories), Vector2DJagged::F64(probabilities), ArrayND::Str(null)) =>
                    {
                        let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<String>>>();
                        let probabilities = probabilities.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
                        return Ok(Value::ArrayND(ArrayND::Str(utilities::transformations::impute_categorical(&data, &categories, &probabilities, &null))));
                    },
                _ => return Err("types of data, categories, and null must be consistent and probabilities must be f64".to_string())
            },
            _ => return Err("data and null must be ArrayND, categories and probabilities must be Vector2DJagged".to_string())
        }
    } else {
        let distribution = match arguments.get("distribution").unwrap() {
            Value::ArrayND(array) => match array {
                ArrayND::Str(distribution) => distribution.first().unwrap(),
                _ => return Err("distribution must be a string".to_string())
            },
            _ => return Err("distribution must be wrapped in an ArrayND".to_string())
        };

        match (distribution) {
            Uniform => {
                match (arguments.get("data").unwrap(), arguments.get("min").unwrap(), arguments.get("max").unwrap()) {
                    (Value::ArrayND(data), Value::ArrayND(min), Value::ArrayND(max))
                        => match (data, min, max) {
                            (ArrayND::F64(data), ArrayND::F64(min), ArrayND::F64(max))
                                => return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::impute_numeric(
                                             &data, &distribution, &Float, &min, &max, &None, &None)))),
                            (ArrayND::I64(data), ArrayND::I64(min), ArrayND::I64(max))
                                => return Ok(Value::ArrayND(ArrayND::I64(data.clone()))),
                            _ => return Err("data, min, and max must all be the same type".to_string())
                        }
                    _ => return Err("data, min, max, shift, and scale must be ArrayND".to_string())
                }
            },
            Gaussian => {
                match (arguments.get("data").unwrap(), arguments.get("min").unwrap(),
                       arguments.get("max").unwrap(), arguments.get("shift").unwrap(), arguments.get("scale").unwrap()) {
                    (Value::ArrayND(data), Value::ArrayND(min), Value::ArrayND(max), Value::ArrayND(shift), Value::ArrayND(scale))
                        => match(data, min, max, shift, scale) {
                            (ArrayND::F64(data), ArrayND::F64(min), ArrayND::F64(max), ArrayND::F64(shift), ArrayND::F64(scale))
                                => return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::impute_numeric(
                                             &data, &distribution, &Float, &min, &max, &Some(shift.to_owned()), &Some(scale.to_owned()))))),
                            _ => return Err("data, min, max, shift, and scale must all be f64".to_string())
                        },
                    _ =>
                        return Err("data, min, max, shift, and scale must all be ArrayND".to_string())
                };
            },
            _ => return Err("Distribution not supported".to_string())
        }
        // match (arguments.get("data").unwrap(), arguments.get("distribution").unwrap(), arguments.get("data_type").unwrap(),
            //    arguments.get("min").unwrap(), arguments.get("max").unwrap(),
            //    arguments.get("shift").unwrap(), arguments.get("scale").unwrap()) {
            // (Value::ArrayND(data), Value::ArrayND(distribution), Value::ArrayND(data_type),
            //  Value::ArrayND(min), Value::ArrayND(max), Value::Vector1DNull(shift), Value::Vector1DNull(scale)) => match(data, distribution, data_type, min, max, shift, scale) {
                // (ArrayND::F64(data), ArrayND::Str(distribution), ArrayND::Str(data_type),
                //  ArrayND::F64(min), ArrayND::F64(max), Vector1DNull::F64(shift), Vector1DNull::F64(scale)) =>
                //     return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::impute_numeric(&data, &distribution, &data_type, &min, &max, &shift, &scale)))),
                // // (ArrayND::I64(data), ArrayND::I64(min), ArrayND::I64(max)) =>
                //     // return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::impute_numeric(&data, &distribution, &data_type, &min, &max, &shift, &scale)))),
                // _ => return Err("data, min, max, shift, and scale must all be f64 -- distribution and data_type must be String".to_string())
            // },
            // _ => return Err("data, distribution, data_type, min, and max must all be ArrayND -- shift and scale must be Vector1DNull".to_string())
    }
}


// // TODO: still working on this
// pub fn component_impute(_x: &proto::Impute, arguments: &NodeArguments,) -> Result<Value, String> {
//     if arguments.contains_key("categories") {
//         match (arguments.get("data").unwrap(), arguments.get("categories").unwrap(), arguments.get("probabilities").unwrap(), arguments.get("null").unwrap()) {
//             (Value::ArrayND(data), Value::Vector2DJagged(categories), Value::Vector2DJagged(probabilities), Value::ArrayND(null)) => match (data, categories, probabilities, null) {
//                 (ArrayND::Bool(data), Vector2DJagged::Bool(categories), Vector2DJagged::F64(probabilities), ArrayND::Bool(null)) =>
//                     {
//                         let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<bool>>>();
//                         let probabilities = probabilities.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
//                         return Ok(Value::ArrayND(ArrayND::Bool(utilities::transformations::impute_categorical(&data, &categories, &probabilities, &null))));
//                     },
//                 (ArrayND::F64(data), Vector2DJagged::F64(categories), Vector2DJagged::F64(probabilities), ArrayND::F64(null)) =>
//                     {
//                         let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
//                         let probabilities = probabilities.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
//                         return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::impute_categorical(&data, &categories, &probabilities, &null))));
//                     },
//                 (ArrayND::I64(data), Vector2DJagged::I64(categories), Vector2DJagged::F64(probabilities), ArrayND::I64(null)) =>
//                     {
//                         let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<i64>>>();
//                         let probabilities = probabilities.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
//                         return Ok(Value::ArrayND(ArrayND::I64(utilities::transformations::impute_categorical(&data, &categories, &probabilities, &null))));
//                     },
//                 (ArrayND::Str(data), Vector2DJagged::Str(categories), Vector2DJagged::F64(probabilities), ArrayND::Str(null)) =>
//                     {
//                         let categories = categories.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<String>>>();
//                         let probabilities = probabilities.iter().map(|column| column.to_owned().unwrap()).collect::<Vec<Vec<f64>>>();
//                         return Ok(Value::ArrayND(ArrayND::Str(utilities::transformations::impute_categorical(&data, &categories, &probabilities, &null))));
//                     },
//                 _ => return Err("types of data, categories, and null must be consistent and probabilities must be f64".to_string())
//             },
//             _ => return Err("data and null must be ArrayND, categories and probabilities must be Vector2DJagged".to_string())
//         }
//     } else {
//         match (arguments.get("data").unwrap(), arguments.get("distribution").unwrap(), arguments.get("data_type").unwrap(),
//                arguments.get("min").unwrap(), arguments.get("max").unwrap(),
//                arguments.get("shift").unwrap(), arguments.get("scale").unwrap()) {
//             (Value::ArrayND(data), Value::ArrayND(distribution), Value::ArrayND(data_type),
//              Value::ArrayND(min), Value::ArrayND(max), Value::Vector1DNull(shift), Value::Vector1DNull(scale)) => match(data, distribution, data_type, min, max, shift, scale) {
//                 (ArrayND::F64(data), ArrayND::Str(distribution), ArrayND::Str(data_type),
//                  ArrayND::F64(min), ArrayND::F64(max), Vector1DNull::F64(shift), Vector1DNull::F64(scale)) =>
//                     return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::impute_numeric(&data, &distribution, &data_type, &min, &max, &shift, &scale)))),
//                 // (ArrayND::I64(data), ArrayND::I64(min), ArrayND::I64(max)) =>
//                     // return Ok(Value::ArrayND(ArrayND::F64(utilities::transformations::impute_numeric(&data, &distribution, &data_type, &min, &max, &shift, &scale)))),
//                 _ =>
//                     return Err("data, min, max, shift, and scale must all be f64 -- distribution and data_type must be String".to_string())
//             },
//             _ => return Err("data, distribution, data_type, min, and max must all be ArrayND -- shift and scale must be Vector1DNull".to_string())
//         }
//     }
// }

//pub fn component_count(
//    _X: &proto::Count, arguments: &NodeArguments,
//) -> Result<Value, String> {
//
//    match (arguments.get("data").unwrap(), arguments.get("group_by").unwrap()) {
//        (Value::F64(data), Value::F64(group_by)) =>
//            Ok(Value::F64(utilities::aggregations::count(&get_array_f64(&arguments, "data"), &Some(get_array_f64(&arguments, "group_by"))))),
//        (Value::Str(data), Value::Str(group_by)) =>
//            Ok(Value::F64(utilities::aggregations::count(&get_array_str(&arguments, "data"), &Some(get_array_str(&arguments, "group_by"))))),
//        (Value::Bool(data), Value::Bool(group_by)) =>
//            Ok(Value::F64(utilities::aggregations::count(&get_array_bool(&arguments, "data"), &Some(get_array_bool(&arguments, "group_by"))))),
//        _ => Err("Count: Data type must be f64, string, or bool".to_string())
//    }
//}

//pub fn component_histogram(
//    _X: &proto::Bin, argument: &NodeArguments
//) -> Result<Value, String> {
//    let data: ArrayD<f64> = get_array_f64(&arguments, "data");
//    let edges: ArrayD<f64> = get_array_f64(&arguments, "edges");
//    let inclusive_left: bool = get_bool(&arguments, "inclusive_left");
//    Ok(Value::String_F64_HashMap(utilities::aggregations::histogram(&data, &edges, &inclusive_left))
//}

pub fn component_mean(
    _x: &proto::Mean, arguments: &NodeArguments
) -> Result<Value, String> {

    let data: ArrayD<f64> = get_array_f64(&arguments, "data");
    Ok(Value::ArrayND(ArrayND::F64(utilities::aggregations::mean(&data))))
}

pub fn component_variance(
    _x: &proto::Variance, arguments: &NodeArguments
) -> Result<Value, String> {

    let data: ArrayD<f64> = get_array_f64(&arguments, "data");
    let finite_sample_correction: bool = get_bool(&arguments, "finite_sample_correction");
    Ok(Value::ArrayND(ArrayND::F64(utilities::aggregations::variance(&data, &finite_sample_correction))))
}

pub fn component_kth_raw_sample_moment(
    _x: &proto::KthRawSampleMoment, arguments: &NodeArguments
) -> Result<Value, String> {
    let data: ArrayD<f64> = get_array_f64(&arguments, "data");
    let k: i64 = get_i64(&arguments, "k");
    Ok(Value::ArrayND(ArrayND::F64(utilities::aggregations::kth_raw_sample_moment(&data, &k))))
}

pub fn component_median(
    _x: &proto::Median, arguments: &NodeArguments
) -> Result<Value, String> {
    let data: ArrayD<f64> = get_array_f64(&arguments, "data");
    Ok(Value::ArrayND(ArrayND::F64(utilities::aggregations::median(&data))))
}

pub fn component_sum(
    _x: &proto::Sum, arguments: &NodeArguments
) -> Result<Value, String> {

    let data: ArrayD<f64> = get_array_f64(&arguments, "data");
    Ok(Value::ArrayND(ArrayND::F64(utilities::aggregations::sum(&data))))
}

pub fn component_laplace_mechanism(
    _x: &proto::LaplaceMechanism, arguments: &NodeArguments
) -> Result<Value, String> {
    let epsilon: f64 = get_f64(&arguments, "epsilon");
    let sensitivity: f64 = get_f64(&arguments, "sensitivity");
    Ok(Value::ArrayND(ArrayND::F64(utilities::mechanisms::laplace_mechanism(&epsilon, &sensitivity))))
}

pub fn component_gaussian_mechanism(
    _x: &proto::GaussianMechanism, arguments: &NodeArguments
) -> Result<Value, String> {
    let epsilon: f64 = get_f64(&arguments, "epsilon");
    let delta: f64 = get_f64(&arguments, "delta");
    let sensitivity: f64 = get_f64(&arguments, "sensitivity");
    Ok(Value::ArrayND(ArrayND::F64(utilities::mechanisms::gaussian_mechanism(&epsilon, &delta, &sensitivity))))
}

pub fn component_simple_geometric_mechanism(
    _x: &proto::SimpleGeometricMechanism, arguments: &NodeArguments
) -> Result<Value, String> {
    let epsilon: f64 = get_f64(&arguments, "epsilon");
    let sensitivity: f64 = get_f64(&arguments, "sensitivity");
    let count_min: i64 = get_i64(&arguments, "count_min");
    let count_max: i64 = get_i64(&arguments, "count_max");
    let enforce_constant_time: bool = get_bool(&arguments, "enforce_constant_time");
    Ok(Value::ArrayND(ArrayND::I64(utilities::mechanisms::simple_geometric_mechanism(
                             &epsilon, &sensitivity, &count_min, &count_max, &enforce_constant_time))))
}

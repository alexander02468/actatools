// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::Error;
use polars::prelude::{AnyValue, Scalar};

/// Functions that have to do with conversions of values
///

pub fn convert_scalar_to_bytes_array(s: &Scalar) -> Result<Vec<u8>, Error> {
    convert_anyvalue_to_bytes_array(&s.as_any_value())
}

/// converts polars::anyvalue to serde_json::value
pub fn convert_anyvalue_to_json(v: &AnyValue) -> Result<serde_json::Value, Error> {
    let json_value = match v {
        AnyValue::Null => serde_json::Value::Null,

        AnyValue::Boolean(b) => serde_json::Value::Bool(*b),

        AnyValue::Int8(x) => serde_json::Value::from(*x),
        AnyValue::Int16(x) => serde_json::Value::from(*x),
        AnyValue::Int32(x) => serde_json::Value::from(*x),
        AnyValue::Int64(x) => serde_json::Value::from(*x),

        AnyValue::UInt8(x) => serde_json::Value::from(*x),
        AnyValue::UInt16(x) => serde_json::Value::from(*x),
        AnyValue::UInt32(x) => serde_json::Value::from(*x),
        AnyValue::UInt64(x) => serde_json::Value::from(*x),

        AnyValue::Float32(x) => serde_json::Value::from(*x),

        AnyValue::Float64(x) => serde_json::Value::from(*x),

        AnyValue::String(s) => serde_json::Value::String(s.to_string()),
        AnyValue::StringOwned(s) => serde_json::Value::String(s.to_string()),

        // Binary has no JSON equivalent; encode as base64-ish string, or pick hex.
        // AnyValue::Binary(b) => serde_json::Value::String(base64::encode(b)),
        // AnyValue::BinaryOwned(b) => serde_json::Value::String(base64::encode(b)),

        // // Lists: recurse
        // AnyValue::List(series) => {
        //     let mut out = Vec::with_capacity(series.len());
        //     for i in 0..series.len() {
        //         // get(i) returns AnyValue; Null if out of bounds
        //         out.push(anyvalue_to_json(&series.get(i).unwrap_or(AnyValue::Null))?);
        //     }
        //     JsonValue::Array(out)
        // }

        // // Struct: recurse into fields
        // AnyValue::Struct(_, fields, values) => {
        //     let mut obj = JsonMap::with_capacity(fields.len());
        //     for (name, val) in fields.iter().zip(values.iter()) {
        //         obj.insert(name.to_string(), anyvalue_to_json(val)?);
        //     }
        //     JsonValue::Object(obj)
        // }

        // Date/Datetime/Duration/Time/Decimal/etc:
        // Easiest stable choice: stringify.
        AnyValue::Date(_) | AnyValue::Datetime(_, _, _) | AnyValue::Duration(_, _) => {
            serde_json::Value::String(v.to_string())
        }

        // Anything else Polars adds (Object, etc.): stringify.
        _ => serde_json::Value::String(v.to_string()),
    };
    Ok(json_value)
}

pub fn convert_anyvalue_to_bytes_array(v: &AnyValue) -> Result<Vec<u8>, Error> {
    let mut bytes_vec: Vec<u8> = Vec::with_capacity(16); // 16 bytes for now, will shrink at the end, possibly strings will need to expand

    match v {
        AnyValue::Null => bytes_vec.push(u8::from(0)),

        AnyValue::Boolean(b) => match b {
            true => bytes_vec.push(u8::from(1)),
            false => bytes_vec.push(u8::from(0)),
        },

        AnyValue::Int8(x) => bytes_vec.push(x.clone() as u8),
        AnyValue::Int16(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),
        AnyValue::Int32(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),
        AnyValue::Int64(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),

        AnyValue::UInt8(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),
        AnyValue::UInt16(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),
        AnyValue::UInt32(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),
        AnyValue::UInt64(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),

        AnyValue::Float32(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),

        AnyValue::Float64(x) => bytes_vec.extend_from_slice(&x.clone().to_le_bytes()),

        AnyValue::String(s) => bytes_vec.extend_from_slice(s.as_bytes()),
        AnyValue::StringOwned(s) => bytes_vec.extend_from_slice(s.to_string().as_bytes()),

        // Binary has no JSON equivalent; encode as base64-ish string, or pick hex.
        // AnyValue::Binary(b) => serde_json::Value::String(base64::encode(b)),
        // AnyValue::BinaryOwned(b) => serde_json::Value::String(base64::encode(b)),

        // // Lists: recurse
        // AnyValue::List(series) => {
        //     let mut out = Vec::with_capacity(series.len());
        //     for i in 0..series.len() {
        //         // get(i) returns AnyValue; Null if out of bounds
        //         out.push(anyvalue_to_json(&series.get(i).unwrap_or(AnyValue::Null))?);
        //     }
        //     JsonValue::Array(out)
        // }

        // // Struct: recurse into fields
        // AnyValue::Struct(_, fields, values) => {
        //     let mut obj = JsonMap::with_capacity(fields.len());
        //     for (name, val) in fields.iter().zip(values.iter()) {
        //         obj.insert(name.to_string(), anyvalue_to_json(val)?);
        //     }
        //     JsonValue::Object(obj)
        // }

        // Date/Datetime/Duration/Time/Decimal/etc:
        // Easiest stable choice: stringify.
        AnyValue::Date(_) | AnyValue::Datetime(_, _, _) | AnyValue::Duration(_, _) => {
            bytes_vec.extend_from_slice(v.to_string().as_bytes())
        }

        // Anything else Polars adds (Object, etc.): stringify then add the bytes.
        _ => bytes_vec.extend_from_slice(v.to_string().as_bytes()),
    };
    Ok(bytes_vec)
}

pub fn convert_toml_value_to_scalar(toml_val: &toml::Value) -> Result<Scalar, Error> {
    let scalar = match toml_val {
        toml::Value::String(string) => Ok(Scalar::new(
            polars::prelude::DataType::String,
            AnyValue::String(string).into_static(),
        )),
        toml::Value::Integer(int) => Ok(Scalar::from(*int)),
        toml::Value::Float(f) => Ok(Scalar::from(*f)),
        toml::Value::Boolean(b) => Ok(Scalar::from(*b)),
        toml::Value::Datetime(dt) => Ok(Scalar::from(dt.to_string().into_bytes())),

        other => Err(anyhow::anyhow!(other.to_string())),
    };
    return scalar;
}

pub fn convert_path_to_absolute(input_path: &Path) -> Result<PathBuf, Error> {
    if input_path.is_absolute() {
        Ok(input_path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(input_path))
    }
}

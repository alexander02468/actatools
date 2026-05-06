// Copyright (C) 2026 Alexander Baker
// SPDX-License-Identifier: GPL-3.0-or-later

use polars::prelude::AnyValue;

/// Functions that have to do with conversions of values
///

#[derive(Clone, Debug)]
pub enum ScalarConversionError {
    FileTypeNotSupport,
}

impl std::fmt::Display for ScalarConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Only String scalars are supported")
    }
}

/// converts polars::anyvalue to serde_json::value
/// Only support Strings, Through an error otherwise
/// Plan to remove this in a future refactor
pub fn convert_anyvalue_to_json(v: &AnyValue) -> Result<serde_json::Value, ScalarConversionError> {
    match v {
        AnyValue::String(s) => Ok(serde_json::Value::String(s.to_string())),
        AnyValue::StringOwned(s) => Ok(serde_json::Value::String(s.to_string())),

        // Anything else Polars adds (Object, etc.): stringify.
        _ => Err(ScalarConversionError::FileTypeNotSupport),
    }
}

/// Tests for conversions
#[cfg(test)]
mod test_conversion {

    use super::*;

    #[test]
    fn test_string_json_conversion() {
        let x = polars::prelude::AnyValue::String("foobar");
        let out = convert_anyvalue_to_json(&x).unwrap();

        assert_eq!(out.to_string(), serde_json::to_string("foobar").unwrap());
    }

    #[test]
    fn test_other_json_conversion() {
        let x = polars::prelude::AnyValue::Float64(64.);
        let out = convert_anyvalue_to_json(&x);
        assert!(out.is_err())
    }
}

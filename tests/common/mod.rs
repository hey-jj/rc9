//! Shared helpers for the integration tests.
//!
//! Each test binary compiles this module on its own and uses a different subset
//! of helpers, so unused-item warnings here are expected and allowed.
#![allow(dead_code)]

use serde_json::Value;

/// The nested config object reused across the round-trip tests.
pub fn sample_config() -> Value {
    serde_json::json!({
        "db": {
            "username": "db username",
            "password": "db pass",
            "enabled": false
        }
    })
}

/// Assert that every key and value in `expected` is present in `actual`.
///
/// Recurses into nested objects. Mirrors a subset match where the actual value
/// may carry extra keys that the expected value does not mention.
pub fn assert_matches_subset(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            for (key, exp_val) in e {
                let act_val = a
                    .get(key)
                    .unwrap_or_else(|| panic!("missing key {key:?} in {actual}"));
                assert_matches_subset(act_val, exp_val);
            }
        }
        _ => assert_eq!(actual, expected, "value mismatch"),
    }
}

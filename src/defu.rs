//! Deep merge where the base value wins and the defaults fill the gaps.
//!
//! Used by `update` to combine an incoming config with the config already on
//! disk. The incoming value has priority. The on-disk value supplies anything
//! the incoming value leaves out.
//!
//! Merge rules:
//! - Plain objects merge key by key.
//! - Arrays concatenate with the base elements first, then the defaults.
//! - A scalar in the base overwrites the default.
//! - A `null` value in the base is skipped and does not clobber the default.
//! - Keys `__proto__` and `constructor` in the base are skipped.

use serde_json::{Map, Value};

/// Merge `base` over `defaults`, returning a new value.
///
/// `base` is the higher-priority side. `defaults` fills in missing pieces.
pub fn defu(base: &Value, defaults: &Value) -> Value {
    merge(base, defaults)
}

/// Recursive merge of one base value onto one defaults value.
fn merge(base: &Value, defaults: &Value) -> Value {
    // When the defaults side is not a plain object there is nothing to merge
    // into. Treat it as an empty object so base keys still apply.
    let defaults_map = match defaults {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };

    // When the base side is not a plain object, the result is just the defaults
    // copy. The source only iterates base keys when base is an object.
    let Value::Object(base_map) = base else {
        return Value::Object(defaults_map);
    };

    let mut object = defaults_map;
    for (key, value) in base_map {
        // Prototype-pollution guard on the base keys.
        if key == "__proto__" || key == "constructor" {
            continue;
        }
        // Skip null. Undefined has no representation, so null covers both. An
        // empty string is not skipped and does overwrite.
        if value.is_null() {
            continue;
        }

        match (value, object.get(key)) {
            (Value::Array(base_arr), Some(Value::Array(def_arr))) => {
                // Concatenate base first, then defaults.
                let mut merged = base_arr.clone();
                merged.extend(def_arr.clone());
                object.insert(key.clone(), Value::Array(merged));
            }
            (Value::Object(_), Some(Value::Object(_))) => {
                let def_child = object.get(key).cloned().unwrap_or(Value::Null);
                object.insert(key.clone(), merge(value, &def_child));
            }
            _ => {
                // Scalar or mismatched shapes: the base value wins.
                object.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(object)
}

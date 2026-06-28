//! Flatten and unflatten a value tree using `.` as the key delimiter.
//!
//! `flatten` turns nested objects and arrays into a single-level map of dotted
//! keys. `unflatten` reverses it and rebuilds arrays from numeric path segments.
//! Both preserve insertion order.

use serde_json::{Map, Value};

/// Flatten a nested value into a single-level map of dotted keys.
///
/// Non-empty objects and arrays recurse and build dotted keys. Array elements
/// use their numeric index as the key segment, so `{a:["x","y"]}` becomes
/// `{"a.0":"x","a.1":"y"}`. Empty objects and arrays stay as leaf values.
/// Scalars are leaves.
///
/// Input that is not an object returns an empty map, which matches the source
/// when given a non-object top-level value.
pub fn flatten(target: &Value) -> Map<String, Value> {
    let mut output = Map::new();
    if let Value::Object(_) | Value::Array(_) = target {
        step(target, None, &mut output);
    }
    output
}

/// Walk one level of the tree, emitting leaves into `output`.
fn step(value: &Value, prev: Option<&str>, output: &mut Map<String, Value>) {
    let entries: Vec<(String, &Value)> = match value {
        Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v)).collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        _ => return,
    };

    for (key, child) in entries {
        let new_key = match prev {
            Some(p) => format!("{p}.{key}"),
            None => key,
        };
        if is_non_empty_container(child) {
            step(child, Some(&new_key), output);
        } else {
            output.insert(new_key, child.clone());
        }
    }
}

/// Whether a value is an object or array with at least one entry.
fn is_non_empty_container(value: &Value) -> bool {
    match value {
        Value::Object(map) => !map.is_empty(),
        Value::Array(items) => !items.is_empty(),
        _ => false,
    }
}

/// A resolved path segment: an array index or an object key.
enum Key {
    Index(usize),
    Name(String),
}

/// Decide whether a segment is an array index or an object key.
///
/// A segment is an array index when it parses as a finite number and contains no
/// dot. This follows the source rule built on a permissive numeric coercion. The
/// realistic config case is a plain non-negative integer.
fn get_key(segment: &str) -> Key {
    if !segment.contains('.') {
        if let Some(n) = js_number_index(segment) {
            return Key::Index(n);
        }
    }
    Key::Name(segment.to_string())
}

/// Parse a segment as an array index, mirroring the permissive numeric coercion.
///
/// Returns `Some(index)` when the segment coerces to a non-negative integer.
/// Handles the forms that appear in config keys: plain decimal integers,
/// empty and whitespace-only strings (which coerce to zero), and surrounding
/// whitespace. Returns `None` for anything that is not a finite non-negative
/// integer index, including floats with a fractional part and negative numbers.
fn js_number_index(segment: &str) -> Option<usize> {
    let trimmed = segment.trim_matches(crate::destr::js_whitespace);
    // An empty or whitespace-only segment coerces to zero.
    if trimmed.is_empty() {
        return Some(0);
    }
    // Reject a leading sign for index use. A negative index is not valid and a
    // positive sign never appears in generated keys.
    if trimmed.starts_with('-') || trimmed.starts_with('+') {
        return None;
    }
    let n: f64 = trimmed.parse().ok()?;
    if n.is_finite() && n.fract() == 0.0 && n >= 0.0 {
        Some(n as usize)
    } else {
        None
    }
}

/// Rebuild a nested value from a single-level map of dotted keys.
///
/// Numeric path segments create arrays. Other segments create objects. With
/// overwrite enabled, a deeper path replaces a shallower scalar at the same
/// prefix, so flat keys `x` and `x.y` resolve to `{x:{y:...}}`. A path that
/// contains a `__proto__` segment is skipped.
///
/// Input that is not an object returns unchanged.
pub fn unflatten(target: &Value) -> Value {
    let Value::Object(input) = target else {
        return target.clone();
    };

    // Pre-pass: re-flatten any nested non-empty container values so their inner
    // keys join the dotted keyspace. Leaf values keep their key.
    let mut expanded: Map<String, Value> = Map::new();
    for (key, value) in input {
        if is_non_empty_container(value) {
            for (sub_key, sub_val) in flatten(value) {
                expanded.insert(format!("{key}.{sub_key}"), sub_val);
            }
        } else {
            expanded.insert(key.clone(), value.clone());
        }
    }

    let mut result = Value::Object(Map::new());
    for (key, original_value) in &expanded {
        let segments: Vec<&str> = key.split('.').collect();
        place(&mut result, &segments, original_value);
    }
    result
}

/// Insert a value at a dotted path, creating arrays or objects as needed.
///
/// Containers for segments left of a `__proto__` segment are created, then the
/// walk stops. A `__proto__` segment is never written as a key, whether it is an
/// intermediate or the final segment. This matches the guard that drops the key
/// while leaving the containers built up to that point.
fn place(root: &mut Value, segments: &[&str], leaf: &Value) {
    let mut current = root;
    for (i, segment) in segments.iter().enumerate() {
        // Stop before creating or assigning a `__proto__` segment.
        if *segment == "__proto__" {
            return;
        }

        let key = get_key(segment);
        let is_last = i == segments.len() - 1;

        if is_last {
            set_child(current, &key, unflatten(leaf));
            return;
        }

        // Decide the container type for the next level from the next segment.
        let next_is_index = matches!(get_key(segments[i + 1]), Key::Index(_));
        current = descend(current, &key, next_is_index);
    }
}

/// Move into the child at `key`, creating a fresh container when needed.
///
/// With overwrite semantics, a non-container value at `key` is replaced by a
/// fresh array or object so the deeper path can be written.
fn descend<'a>(current: &'a mut Value, key: &Key, next_is_index: bool) -> &'a mut Value {
    let needs_new = !child_is_container(current, key);
    if needs_new {
        let fresh = if next_is_index {
            Value::Array(Vec::new())
        } else {
            Value::Object(Map::new())
        };
        set_child(current, key, fresh);
    }
    child_mut(current, key).expect("child was just ensured to exist")
}

/// Whether the child at `key` is an object or array.
fn child_is_container(current: &Value, key: &Key) -> bool {
    match (current, key) {
        (Value::Object(map), Key::Name(name)) => {
            matches!(map.get(name), Some(Value::Object(_) | Value::Array(_)))
        }
        (Value::Object(map), Key::Index(idx)) => {
            matches!(
                map.get(&idx.to_string()),
                Some(Value::Object(_) | Value::Array(_))
            )
        }
        (Value::Array(items), Key::Index(idx)) => {
            matches!(items.get(*idx), Some(Value::Object(_) | Value::Array(_)))
        }
        _ => false,
    }
}

/// Set the child at `key`, growing an array with nulls if the index is past its end.
fn set_child(current: &mut Value, key: &Key, value: Value) {
    match (current, key) {
        (Value::Object(map), Key::Name(name)) => {
            map.insert(name.clone(), value);
        }
        (Value::Object(map), Key::Index(idx)) => {
            // A numeric key against an existing object stays a string key.
            map.insert(idx.to_string(), value);
        }
        (Value::Array(items), Key::Index(idx)) => {
            if *idx >= items.len() {
                items.resize(*idx + 1, Value::Null);
            }
            items[*idx] = value;
        }
        (Value::Array(_), Key::Name(_)) => {
            // A name segment against an array cannot happen with generated keys.
        }
        _ => {
            // The current value is a scalar. Generated paths always create a
            // container before reaching here, so this case does not occur.
        }
    }
}

/// Borrow the child at `key` mutably.
fn child_mut<'a>(current: &'a mut Value, key: &Key) -> Option<&'a mut Value> {
    match (current, key) {
        (Value::Object(map), Key::Name(name)) => map.get_mut(name),
        (Value::Object(map), Key::Index(idx)) => map.get_mut(&idx.to_string()),
        (Value::Array(items), Key::Index(idx)) => items.get_mut(*idx),
        _ => None,
    }
}

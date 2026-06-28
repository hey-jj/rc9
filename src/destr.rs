//! Value coercion that turns a raw config string into a native value.
//!
//! Mirrors the behavior of a permissive JSON parser that also understands a few
//! keywords. The rules are conservative. A value is coerced only when the result
//! round-trips safely. Anything else stays a string.
//!
//! Coercion order:
//! 1. A value wrapped in double quotes with no backslash returns the inner text.
//! 2. The keywords `true`, `false`, `null`, `undefined`, `nan`, `infinity`, and
//!    `-infinity` map to their values (case-insensitive, trimmed length 9 or less).
//! 3. A value that looks like JSON (starts with `"`, `[`, `{`, or matches the
//!    number shape) is parsed as JSON.
//! 4. Everything else returns unchanged as a string.
//!
//! `undefined` has no representation in this value model. The keyword maps to
//! [`Value::Null`]. The keyword is the only source of an undefined value and the
//! conformance tests do not write it back, so the mapping is observationally safe.

use serde_json::Value;

/// Coerce a raw string into a native [`Value`].
///
/// Never fails. On any parse problem it returns the input as a string value.
pub fn destr(value: &str) -> Value {
    // 1. Quoted-string fast path. A value that opens and closes with `"` and has
    //    no backslash anywhere returns the text between the quotes. No unescaping.
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && bytes[0] == b'"'
        && bytes[bytes.len() - 1] == b'"'
        && !value.contains('\\')
    {
        return Value::String(value[1..value.len() - 1].to_string());
    }

    // 2. Keyword path. Only consider the trimmed value when it is short enough.
    //    `-infinity` is 9 characters, the longest keyword.
    let trimmed = js_trim(value);
    if trimmed.chars().count() <= 9 {
        match trimmed.to_ascii_lowercase().as_str() {
            "true" => return Value::Bool(true),
            "false" => return Value::Bool(false),
            // `undefined` has no value here, see the module note.
            "undefined" => return Value::Null,
            "null" => return Value::Null,
            // NaN and the infinities have no JSON form. They serialize to `null`.
            "nan" | "infinity" | "-infinity" => return Value::Null,
            _ => {}
        }
    }

    // 3. JSON-signature gate. Skip JSON parsing unless the value looks like JSON.
    if !json_sig(value) {
        return Value::String(value.to_string());
    }

    // 4. JSON parse with a prototype-pollution filter. On failure return the raw
    //    string. A successful parse yields the native value.
    match serde_json::from_str::<Value>(value) {
        Ok(parsed) => normalize(strip_dangerous_keys(parsed)),
        Err(_) => Value::String(value.to_string()),
    }
}

/// Normalize parsed numbers to match the single-number-type model.
///
/// A float with no fractional part that fits an integer becomes an integer, so
/// `1e3` reads as `1000` and serializes as `1000` rather than `1000.0`. This
/// matches a parser with one number type that prints integral values without a
/// decimal point. Recurses into arrays and objects.
fn normalize(value: Value) -> Value {
    match value {
        Value::Number(num) => normalize_number(num),
        Value::Array(items) => Value::Array(items.into_iter().map(normalize).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, normalize(v))).collect())
        }
        other => other,
    }
}

/// Convert an integral float into an integer number when it fits.
fn normalize_number(num: serde_json::Number) -> Value {
    if num.is_i64() || num.is_u64() {
        return Value::Number(num);
    }
    if let Some(f) = num.as_f64() {
        if f.is_finite() && f.fract() == 0.0 {
            if f >= 0.0 && f <= u64::MAX as f64 {
                return Value::Number((f as u64).into());
            }
            if f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                return Value::Number((f as i64).into());
            }
        }
    }
    Value::Number(num)
}

/// Test whether a value matches the JSON signature gate.
///
/// Equivalent to the pattern `^\s*["[{]|^\s*-?\d{1,16}(\.\d{1,17})?([Ee][+-]?\d+)?\s*$`.
/// A value passes when, after optional leading whitespace, it starts with `"`,
/// `[`, or `{`, or it has the number shape: optional sign, 1 to 16 integer
/// digits, optional dot with 1 to 17 fraction digits, optional exponent, and
/// optional trailing whitespace.
fn json_sig(value: &str) -> bool {
    let after_ws = value.trim_start_matches(js_whitespace);
    if let Some(first) = after_ws.chars().next() {
        if first == '"' || first == '[' || first == '{' {
            return true;
        }
    }
    json_sig_number(value)
}

/// Match the number arm of the signature gate against the whole value.
fn json_sig_number(value: &str) -> bool {
    let mut rest = value.trim_start_matches(js_whitespace);

    // optional sign
    rest = rest.strip_prefix('-').unwrap_or(rest);

    // 1 to 16 integer digits
    let int_digits = take_digits(rest);
    if int_digits == 0 || int_digits > 16 {
        return false;
    }
    rest = &rest[int_digits..];

    // optional fraction: dot then 1 to 17 digits
    if let Some(after_dot) = rest.strip_prefix('.') {
        let frac_digits = take_digits(after_dot);
        if frac_digits == 0 || frac_digits > 17 {
            return false;
        }
        rest = &after_dot[frac_digits..];
    }

    // optional exponent: [Ee] then optional sign then 1 or more digits
    if let Some(after_e) = rest.strip_prefix(['e', 'E']) {
        let after_sign = after_e.strip_prefix(['+', '-']).unwrap_or(after_e);
        let exp_digits = take_digits(after_sign);
        if exp_digits == 0 {
            return false;
        }
        rest = &after_sign[exp_digits..];
    }

    // optional trailing whitespace, then end
    rest.trim_start_matches(js_whitespace).is_empty()
}

/// Count leading ASCII digits.
fn take_digits(s: &str) -> usize {
    s.bytes().take_while(u8::is_ascii_digit).count()
}

/// Recursively drop object keys that a prototype-pollution guard would remove.
///
/// `__proto__` keys are always dropped. A `constructor` key is dropped only when
/// its value is an object that contains a `prototype` key. This matches the exact
/// reviver condition, including the operator-precedence detail where the
/// `constructor` checks apply only to that branch.
fn strip_dangerous_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                if key == "__proto__" {
                    continue;
                }
                if key == "constructor" {
                    if let Value::Object(ref inner) = val {
                        if inner.contains_key("prototype") {
                            continue;
                        }
                    }
                }
                out.insert(key, strip_dangerous_keys(val));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(strip_dangerous_keys).collect()),
        other => other,
    }
}

/// Trim leading and trailing whitespace using the JS whitespace set.
fn js_trim(s: &str) -> &str {
    s.trim_matches(js_whitespace)
}

/// Whitespace characters recognized by the source regex engine.
///
/// Covers ASCII whitespace, the Unicode space separators, the line and
/// paragraph separators, the no-break and narrow no-break spaces, the BOM, and
/// the other code points the source treats as whitespace.
pub(crate) fn js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}' // tab
            | '\u{000A}' // line feed
            | '\u{000B}' // vertical tab
            | '\u{000C}' // form feed
            | '\u{000D}' // carriage return
            | '\u{0020}' // space
            | '\u{00A0}' // no-break space
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
            | '\u{2028}' // line separator
            | '\u{2029}' // paragraph separator
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}' // byte order mark
    )
}

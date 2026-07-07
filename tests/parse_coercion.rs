//! Value coercion matrix. Each value is checked through `parse` of `k=<value>`.
//!
//! Expected results were verified against the canonical coercion behavior.

use serde_json::{json, Value};

/// Parse `k=<value>` and return the coerced value at `k`.
fn coerce(value: &str) -> Value {
    let line = format!("k={value}");
    let parsed = rc9::parse(&line, &rc9::RcOptions::default());
    parsed.get("k").cloned().unwrap_or(Value::Null)
}

#[test]
fn plain_words_stay_strings() {
    assert_eq!(coerce("bar"), json!("bar"));
    assert_eq!(coerce("baz"), json!("baz"));
    assert_eq!(coerce("multi word"), json!("multi word"));
}

#[test]
fn integers_and_floats() {
    assert_eq!(coerce("123"), json!(123));
    assert_eq!(coerce("12.5"), json!(12.5));
    assert_eq!(coerce("123.45"), json!(123.45));
    assert_eq!(coerce("-7"), json!(-7));
    assert_eq!(coerce("0"), json!(0));
}

#[test]
fn exponent_form() {
    // 1e3 parses to 1000.
    assert_eq!(coerce("1e3"), json!(1000));
}

#[test]
fn float_at_u64_rounding_boundary_stays_float() {
    let value = coerce("1.8446744073709552e19");
    let number = value.as_number().unwrap();

    assert_eq!(number.as_u64(), None);
    assert_eq!(number.as_f64(), Some(1.8446744073709552e19));
}

#[test]
fn booleans_and_null() {
    assert_eq!(coerce("true"), json!(true));
    assert_eq!(coerce("false"), json!(false));
    assert_eq!(coerce("null"), Value::Null);
}

#[test]
fn empty_value_is_empty_string() {
    // `k=` yields an empty string, not null and not undefined.
    assert_eq!(coerce(""), json!(""));
}

#[test]
fn quoted_values_stay_strings() {
    assert_eq!(coerce("\"123\""), json!("123"));
    assert_eq!(coerce("\"hello\""), json!("hello"));
}

#[test]
fn json_object_and_array() {
    assert_eq!(coerce("{\"a\":1}"), json!({ "a": 1 }));
    assert_eq!(coerce("[1,2,3]"), json!([1, 2, 3]));
    assert_eq!(coerce("[1,2,\"three\"]"), json!([1, 2, "three"]));
}

#[test]
fn leading_zero_stays_string() {
    // A leading zero is valid by the number shape but invalid JSON, so it falls
    // back to the raw string.
    assert_eq!(coerce("0123"), json!("0123"));
    assert_eq!(coerce("01"), json!("01"));
}

#[test]
fn sixteen_digit_int_parses_seventeen_does_not() {
    assert_eq!(coerce("1234567890123456"), json!(1_234_567_890_123_456_i64));
    assert_eq!(coerce("12345678901234567"), json!("12345678901234567"));
}

#[test]
fn non_number_shapes_stay_strings() {
    assert_eq!(coerce("0x1F"), json!("0x1F"));
    assert_eq!(coerce("+5"), json!("+5"));
    assert_eq!(coerce(".5"), json!(".5"));
}

#[test]
fn nan_and_infinity_become_null() {
    // Non-finite keywords have no JSON form and read as null.
    assert_eq!(coerce("nan"), Value::Null);
    assert_eq!(coerce("NaN"), Value::Null);
    assert_eq!(coerce("infinity"), Value::Null);
    assert_eq!(coerce("-infinity"), Value::Null);
}

#[test]
fn undefined_keyword_reads_as_null() {
    // Undefined has no representation in this value model and maps to null.
    assert_eq!(coerce("undefined"), Value::Null);
}

#[test]
fn keyword_case_insensitive() {
    assert_eq!(coerce("TRUE"), json!(true));
    assert_eq!(coerce("False"), json!(false));
    assert_eq!(coerce("NULL"), Value::Null);
}

//! Edge-case parity pins derived from the canonical coercion and merge behavior.
//!
//! Each expected value here was checked against the canonical runtime, not
//! guessed. The passing tests guard behavior that already matches. The ignored
//! tests record a known divergence with its canonical expected value and the
//! tracking issue, so each flips to passing once the divergence is fixed.

use serde_json::{json, Value};

/// Parse `k=<value>` and return the coerced value at `k`.
fn coerce(value: &str) -> Value {
    let line = format!("k={value}");
    rc9::parse(&line, &rc9::RcOptions::default())
        .get("k")
        .cloned()
        .unwrap_or(Value::Null)
}

// --- Verified-correct pins ---

#[test]
fn negative_zero_reads_as_zero() {
    assert_eq!(coerce("-0"), json!(0));
}

#[test]
fn zero_exponent_reads_as_zero() {
    assert_eq!(coerce("0e0"), json!(0));
}

#[test]
fn integral_float_reads_as_integer() {
    // 1.0 collapses to 1 under a single number model.
    assert_eq!(coerce("1.0"), json!(1));
}

#[test]
fn finite_large_exponent_stays_number() {
    // 1e308 is within the double range and stays a finite number.
    assert!(coerce("1e308").is_number());
}

#[test]
fn seventeen_fraction_digits_parse() {
    // The fraction cap is 17 digits, so this parses to a finite number.
    assert_eq!(coerce("1.12345678901234567"), json!(1.123456789012345_7));
}

#[test]
fn eighteen_fraction_digits_stay_string() {
    // 18 fraction digits exceed the cap and fall back to the raw string.
    assert_eq!(coerce("1.123456789012345678"), json!("1.123456789012345678"));
}

#[test]
fn trailing_junk_after_number_stays_string() {
    assert_eq!(coerce("123abc"), json!("123abc"));
}

#[test]
fn whitespace_only_value_after_quote_strip_stays_string() {
    // A value of three spaces survives as the empty string, since the regex
    // strips leading value whitespace and the trim removes the rest.
    assert_eq!(coerce("   "), json!(""));
}

#[test]
fn cr_only_separator_splits_lines() {
    let parsed = rc9::parse("a=1\rb=2", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "a": 1, "b": 2 }));
}

#[test]
fn numeric_top_level_keys_stay_object_keys() {
    // A numeric first segment does not make the root an array.
    let parsed = rc9::parse("0=a\n1=b", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "0": "a", "1": "b" }));
}

#[test]
fn sparse_numeric_segment_pads_with_null() {
    let parsed = rc9::parse("a.2=c", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "a": [null, null, "c"] }));
}

#[test]
fn array_index_collision_with_name_keeps_array() {
    // x.0 makes an array, then x.y cannot write a name into an array, so it is
    // dropped and the array survives.
    let parsed = rc9::parse("x.0=a\nx.y=b", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "x": ["a"] }));
}

#[test]
fn nested_then_scalar_scalar_wins() {
    let parsed = rc9::parse("x.y=2\nx=1", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "x": 1 }));
}

#[test]
fn double_bracket_strips_only_last_suffix() {
    // Only the final [] is stripped. The key becomes k[] holding an array.
    let parsed = rc9::parse("k[][]=A", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "k[]": ["A"] }));
}

#[test]
fn bracket_in_key_middle_is_a_literal_key() {
    let parsed = rc9::parse("a[]b=1", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "a[]b": 1 }));
}

#[test]
fn two_quote_chars_read_as_empty_string() {
    assert_eq!(coerce("\"\""), json!(""));
}

#[test]
fn serialize_top_level_array_uses_index_keys() {
    assert_eq!(rc9::serialize(&json!(["a", "b"])), "0=\"a\"\n1=\"b\"");
}

// --- Known divergences. Each encodes the canonical expected value. ---

#[test]
#[ignore = "issue #6: [] push onto an existing non-array value is dropped"]
fn push_onto_existing_string_concatenates() {
    // Canonical: "hello".concat("A") -> "helloA".
    let parsed = rc9::parse("k=hello\nk[]=A", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "k": "helloA" }));
}

#[test]
#[ignore = "issue #6: [] push onto a falsy value should start a fresh array"]
fn push_onto_null_starts_array() {
    // Canonical: (null || []).concat("A") -> ["A"].
    let parsed = rc9::parse("k=null\nk[]=A", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "k": ["A"] }));
}

#[test]
#[ignore = "issue #7: lone quote character should strip to empty string"]
fn lone_quote_strips_to_empty() {
    assert_eq!(coerce("\""), json!(""));
}

#[test]
#[ignore = "issue #9: exponent overflow should coerce to a non-finite value"]
fn exponent_overflow_coerces_to_null() {
    // Canonical destr("1e400") -> Infinity, which serializes to null.
    assert_eq!(coerce("1e400"), Value::Null);
    assert_eq!(rc9::serialize(&json!({ "k": coerce("1e400") })), "k=null");
}

#[test]
#[ignore = "issue #10: undefined keyword drops the key in the canonical form"]
fn undefined_keyword_drops_key() {
    let parsed = rc9::parse("a=undefined\nb=1", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "b": 1 }));
}

#[test]
#[ignore = "issue #13: serialize of negative zero should print a bare 0"]
fn serialize_negative_zero_is_bare_zero() {
    // Canonical JSON.stringify(-0) -> "0".
    assert_eq!(rc9::serialize(&json!({ "x": -0.0 })), "x=0");
}

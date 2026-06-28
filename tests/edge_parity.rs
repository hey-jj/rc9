//! Edge-case parity pins derived from the canonical coercion and merge behavior.
//!
//! Each expected value here was checked against the canonical runtime, not
//! guessed. Most tests guard behavior that matches the canonical form. A few pin
//! a deliberate divergence where this crate's value model has no equivalent, with
//! a comment recording what the canonical form does.

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
    let value = "1.123456789012345678";
    assert_eq!(coerce(value), json!(value));
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

// --- Array-push concatenation matching `(existing || []).concat(value)`. ---

#[test]
fn push_onto_existing_string_concatenates() {
    // A non-empty string supports string concat: "hello".concat("A") -> "helloA".
    let parsed = rc9::parse("k=hello\nk[]=A", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "k": "helloA" }));
}

#[test]
fn push_onto_null_starts_array() {
    // A falsy existing value starts fresh: (null || []).concat("A") -> ["A"].
    let parsed = rc9::parse("k=null\nk[]=A", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "k": ["A"] }));
}

#[test]
fn push_onto_empty_string_starts_array() {
    // An empty string is falsy, so the push starts a fresh array.
    let parsed = rc9::parse("k=\nk[]=A", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "k": ["A"] }));
}

#[test]
fn lone_quote_strips_to_empty() {
    // A single `"` has its first and last byte equal, so the quoted-string fast
    // path strips it to an empty string.
    assert_eq!(coerce("\""), json!(""));
}

#[test]
fn exponent_overflow_coerces_to_null() {
    // A number that overflows the double range reads as an infinity, which has
    // no JSON form and serializes to null.
    assert_eq!(coerce("1e400"), Value::Null);
    assert_eq!(coerce("-1e400"), Value::Null);
    assert_eq!(rc9::serialize(&json!({ "k": coerce("1e400") })), "k=null");
}

#[test]
fn serialize_negative_zero_is_bare_zero() {
    // A negative zero prints as a bare 0, matching JSON.stringify(-0) -> "0".
    assert_eq!(rc9::serialize(&json!({ "x": -0.0 })), "x=0");
}

// --- Deliberate divergences where the value model has no equivalent. ---

#[test]
fn undefined_keyword_keeps_key_as_null() {
    // The canonical form returns the JS undefined for `undefined`, and a key
    // whose value is undefined drops out of the parsed object. This value model
    // has no undefined, so the key stays with a null value. Pin both keys to
    // record the divergence: the canonical result is {"b":1}.
    let parsed = rc9::parse("a=undefined\nb=1", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "a": null, "b": 1 }));
}

#[test]
fn large_numeric_segment_becomes_object_key() {
    // A numeric segment past the array-index bound becomes an object key instead
    // of growing the backing array, so one line cannot force a huge allocation.
    let parsed = rc9::parse("a.1000000=x", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "a": { "1000000": "x" } }));
}

#[test]
fn signed_and_radix_segments_become_object_keys() {
    // The canonical numeric coercion accepts a leading sign and the 0x/0b radix
    // prefixes. This crate treats path segments as indices only in plain decimal
    // form, so these become object keys. Canonical: a.-1 yields an empty array,
    // a.0x10 an index-16 array, a.0b10 an index-2 array.
    let signed = rc9::parse("a.-1=x", &rc9::RcOptions::default());
    assert_eq!(signed, json!({ "a": { "-1": "x" } }));
    let hex = rc9::parse("a.0x10=x", &rc9::RcOptions::default());
    assert_eq!(hex, json!({ "a": { "0x10": "x" } }));
    let bin = rc9::parse("a.0b10=x", &rc9::RcOptions::default());
    assert_eq!(bin, json!({ "a": { "0b10": "x" } }));
}

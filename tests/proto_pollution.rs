//! Prototype-pollution guards across the three layers that filter dangerous keys.

use serde_json::json;

#[test]
fn parse_drops_proto_key() {
    let parsed = rc9::parse("__proto__=evil\nok=1", &rc9::RcOptions::default());
    assert!(parsed.get("__proto__").is_none());
    assert_eq!(parsed["ok"], json!(1));
}

#[test]
fn parse_drops_constructor_key() {
    let parsed = rc9::parse("constructor=evil\nok=1", &rc9::RcOptions::default());
    assert!(parsed.get("constructor").is_none());
    assert_eq!(parsed["ok"], json!(1));
}

#[test]
fn destr_drops_proto_in_json_object() {
    // A JSON object value with a __proto__ key has that key stripped on parse.
    let parsed = rc9::parse(
        "o={\"__proto__\":{\"x\":1},\"a\":2}",
        &rc9::RcOptions::default(),
    );
    assert_eq!(parsed["o"], json!({ "a": 2 }));
}

#[test]
fn destr_drops_constructor_with_prototype() {
    // A constructor key whose value is an object with a prototype key is dropped.
    let parsed = rc9::parse(
        "o={\"constructor\":{\"prototype\":{}}}",
        &rc9::RcOptions::default(),
    );
    assert_eq!(parsed["o"], json!({}));
}

#[test]
fn destr_keeps_constructor_without_prototype() {
    // A constructor key whose value is a plain object stays. The guard only
    // fires when the value is an object that has a prototype key.
    let parsed = rc9::parse("o={\"constructor\":{\"x\":1}}", &rc9::RcOptions::default());
    assert_eq!(parsed["o"], json!({ "constructor": { "x": 1 } }));
}

#[test]
fn unflatten_skips_proto_segment_keeps_prior_container() {
    // A dotted key that walks through __proto__ stops at that segment. The
    // container before it stays, the __proto__ key is never written.
    let parsed = rc9::parse("a.__proto__.x=1\nb=2", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "a": {}, "b": 2 }));
}

#[test]
fn unflatten_skips_leading_proto_segment() {
    // A key that begins with __proto__ creates nothing.
    let parsed = rc9::parse(
        "__proto__.x=1\nb=2",
        &rc9::RcOptions {
            flat: None,
            ..Default::default()
        },
    );
    assert_eq!(parsed, json!({ "b": 2 }));
}

#[test]
fn unflatten_skips_trailing_proto_segment() {
    // A trailing __proto__ segment is not written, but the parent stays.
    let parsed = rc9::parse("a.__proto__=1\nb=2", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "a": {}, "b": 2 }));
}

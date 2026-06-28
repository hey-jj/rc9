//! Exact-byte serialize contracts. The serialized form is a hard interface.

use serde_json::{json, Value};

/// Assert that `serialize` produces exactly `expected` for `input`.
fn check(input: Value, expected: &str) {
    assert_eq!(rc9::serialize(&input), expected);
}

#[test]
fn nested_object_with_string_bool() {
    check(
        json!({ "db": { "username": "db username", "password": "db pass", "enabled": false } }),
        "db.username=\"db username\"\ndb.password=\"db pass\"\ndb.enabled=false",
    );
}

#[test]
fn empty_string_value() {
    check(json!({ "db": { "password": "" } }), "db.password=\"\"");
}

#[test]
fn flat_literal_keys() {
    // Order follows insertion: x then x.y.
    check(json!({ "x": 1, "x.y": 2 }), "x=1\nx.y=2");
}

#[test]
fn nested_array_numeric_keys() {
    check(
        json!({ "x": { "foo": ["A", "B"] } }),
        "x.foo.0=\"A\"\nx.foo.1=\"B\"",
    );
}

#[test]
fn numbers_are_bare() {
    check(json!({ "count": 123 }), "count=123");
    check(json!({ "ratio": 12.5 }), "ratio=12.5");
}

#[test]
fn null_writes_as_null() {
    check(json!({ "x": Value::Null }), "x=null");
}

#[test]
fn forward_slash_not_escaped() {
    check(json!({ "url": "http://a/b" }), "url=\"http://a/b\"");
}

#[test]
fn control_chars_escaped() {
    check(json!({ "s": "tab\tnl\n" }), "s=\"tab\\tnl\\n\"");
}

#[test]
fn empty_object_and_array_are_leaves() {
    check(json!({ "a": {} }), "a={}");
    check(json!({ "a": [] }), "a=[]");
}

#[test]
fn no_trailing_newline() {
    let text = rc9::serialize(&json!({ "a": 1, "b": 2 }));
    assert!(!text.ends_with('\n'));
    assert_eq!(text, "a=1\nb=2");
}

#[test]
fn insertion_order_is_preserved() {
    // Reverse-alphabetical insertion order must survive into the output.
    let input = json!({ "zebra": 1, "alpha": 2, "mango": 3 });
    assert_eq!(rc9::serialize(&input), "zebra=1\nalpha=2\nmango=3");
}

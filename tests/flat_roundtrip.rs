//! Flatten and unflatten behavior, flat mode, line handling, and array push.

use serde_json::json;

#[test]
fn dotted_keys_nest_on_read() {
    let parsed = rc9::parse(
        "db.username=alice\ndb.port=5432",
        &rc9::RcOptions::default(),
    );
    assert_eq!(
        parsed,
        json!({ "db": { "username": "alice", "port": 5432 } })
    );
}

#[test]
fn numeric_segment_builds_array() {
    let parsed = rc9::parse("tags.0=A\ntags.1=B", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "tags": ["A", "B"] }));
}

#[test]
fn flat_true_keeps_literal_keys() {
    let parsed = rc9::parse(
        "x=1\nx.y=2",
        &rc9::RcOptions {
            flat: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(parsed, json!({ "x": 1, "x.y": 2 }));
}

#[test]
fn overwrite_collision_deeper_path_wins() {
    // Without flat mode, x and x.y collide. The deeper path replaces the scalar.
    let parsed = rc9::parse("x=1\nx.y=2", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "x": { "y": 2 } }));
}

#[test]
fn serialize_round_trips_nested() {
    let value = json!({ "a": { "b": { "c": "deep" } }, "n": 42, "flag": true });
    let text = rc9::serialize(&value);
    let back = rc9::parse(&text, &rc9::RcOptions::default());
    assert_eq!(back, value);
}

#[test]
fn flatten_unflatten_inverse_for_arrays() {
    let value = json!({ "x": { "foo": ["A", "B", "C"] } });
    let text = rc9::serialize(&value);
    assert_eq!(text, "x.foo.0=\"A\"\nx.foo.1=\"B\"\nx.foo.2=\"C\"");
    assert_eq!(rc9::parse(&text, &rc9::RcOptions::default()), value);
}

#[test]
fn array_push_multiple() {
    let parsed = rc9::parse("list[]=A\nlist[]=B\nlist[]=C", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "list": ["A", "B", "C"] }));
}

#[test]
fn interleaved_array_push_keeps_insertion_order() {
    // Pushing onto a key again after a later key keeps the key in its original
    // position, so serialize emits it first.
    let parsed = rc9::parse("a[]=1\nb=2\na[]=3", &rc9::RcOptions::default());
    assert_eq!(rc9::serialize(&parsed), "a.0=1\na.1=3\nb=2");
}

#[test]
fn array_push_single_onto_fresh_key() {
    let parsed = rc9::parse("list[]=only", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "list": ["only"] }));
}

#[test]
fn array_push_spreads_array_value() {
    // A JSON array value spreads its elements when pushed.
    let parsed = rc9::parse("list[]=[1,2]\nlist[]=3", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "list": [1, 2, 3] }));
}

#[test]
fn dotted_array_push_nests() {
    let parsed = rc9::parse("x.foo[]=A\nx.foo[]=B", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "x": { "foo": ["A", "B"] } }));
}

#[test]
fn comment_line_skipped() {
    let parsed = rc9::parse("# this is a comment\nkey=val", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "key": "val" }));
}

#[test]
fn line_without_equals_skipped() {
    let parsed = rc9::parse("flag\nkey=val", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "key": "val" }));
    assert!(parsed.get("flag").is_none());
}

#[test]
fn blank_lines_skipped() {
    let parsed = rc9::parse("\n\nkey=val\n\n", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "key": "val" }));
}

#[test]
fn value_may_contain_equals() {
    let parsed = rc9::parse("url=http://a.com/?x=1", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "url": "http://a.com/?x=1" }));
}

#[test]
fn whitespace_trimmed_around_equals() {
    let parsed = rc9::parse("  key  =  value  ", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "key": "value" }));
}

#[test]
fn crlf_and_cr_split_lines() {
    let lf = rc9::parse("a=1\nb=2", &rc9::RcOptions::default());
    let crlf = rc9::parse("a=1\r\nb=2", &rc9::RcOptions::default());
    let cr = rc9::parse("a=1\rb=2", &rc9::RcOptions::default());
    let expected = json!({ "a": 1, "b": 2 });
    assert_eq!(lf, expected);
    assert_eq!(crlf, expected);
    assert_eq!(cr, expected);
}

#[test]
fn last_write_wins_for_duplicate_keys() {
    let parsed = rc9::parse("k=first\nk=second", &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "k": "second" }));
}

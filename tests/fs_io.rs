//! Filesystem behavior: read, write, update, parse_file, and merge precedence.

mod common;

use common::sample_config;
use serde_json::json;
use tempfile::TempDir;

/// Options targeting a directory with the default name.
fn dir_opts(tmp: &TempDir) -> rc9::RcOptions {
    rc9::RcOptions {
        dir: Some(tmp.path().to_string_lossy().into_owned()),
        ..Default::default()
    }
}

#[test]
fn write_then_read_round_trip() {
    let tmp = TempDir::new().unwrap();
    rc9::write(&sample_config(), &dir_opts(&tmp)).unwrap();
    assert_eq!(rc9::read(&dir_opts(&tmp)), sample_config());
}

#[test]
fn read_missing_file_is_empty() {
    let tmp = TempDir::new().unwrap();
    let opts = rc9::RcOptions {
        name: Some(".missing".into()),
        dir: Some(tmp.path().to_string_lossy().into_owned()),
        flat: None,
    };
    assert_eq!(rc9::read(&opts), json!({}));
}

#[test]
fn parse_file_direct() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join(".conf");
    std::fs::write(&path, "name=\"alice\"\nport=8080").unwrap();
    let parsed = rc9::parse_file(&path, &rc9::RcOptions::default());
    assert_eq!(parsed, json!({ "name": "alice", "port": 8080 }));
}

#[test]
fn parse_file_missing_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("does-not-exist");
    assert_eq!(
        rc9::parse_file(&path, &rc9::RcOptions::default()),
        json!({})
    );
}

#[test]
fn write_creates_exact_bytes() {
    let tmp = TempDir::new().unwrap();
    rc9::write(&json!({ "a": 1, "b": "two" }), &dir_opts(&tmp)).unwrap();
    let bytes = std::fs::read_to_string(tmp.path().join(".conf")).unwrap();
    assert_eq!(bytes, "a=1\nb=\"two\"");
}

#[test]
fn update_merges_with_existing_file() {
    let tmp = TempDir::new().unwrap();
    rc9::write(&sample_config(), &dir_opts(&tmp)).unwrap();
    let merged = rc9::update(&json!({ "db.password": "updated" }), &dir_opts(&tmp)).unwrap();
    assert_eq!(merged["db"]["password"], json!("updated"));
    // Sibling keys survive the merge.
    assert_eq!(merged["db"]["username"], json!("db username"));
    assert_eq!(merged["db"]["enabled"], json!(false));
    // The file on disk reflects the merge.
    let read = rc9::read(&dir_opts(&tmp));
    assert_eq!(read["db"]["password"], json!("updated"));
}

#[test]
fn update_returns_merged_value() {
    let tmp = TempDir::new().unwrap();
    let opts = rc9::RcOptions {
        name: Some(".new".into()),
        dir: Some(tmp.path().to_string_lossy().into_owned()),
        flat: None,
    };
    // No existing file. The incoming config becomes the whole result.
    let merged = rc9::update(&json!({ "a.b": 1 }), &opts).unwrap();
    assert_eq!(merged, json!({ "a": { "b": 1 } }));
}

#[test]
fn update_incoming_wins_over_file() {
    let tmp = TempDir::new().unwrap();
    rc9::write(&json!({ "x": "old", "keep": "me" }), &dir_opts(&tmp)).unwrap();
    let merged = rc9::update(&json!({ "x": "new" }), &dir_opts(&tmp)).unwrap();
    assert_eq!(merged["x"], json!("new"));
    assert_eq!(merged["keep"], json!("me"));
}

#[test]
fn update_arrays_concatenate_incoming_first() {
    let tmp = TempDir::new().unwrap();
    rc9::write(&json!({ "arr": [1, 2] }), &dir_opts(&tmp)).unwrap();
    let merged = rc9::update(&json!({ "arr": [3, 4] }), &dir_opts(&tmp)).unwrap();
    assert_eq!(merged["arr"], json!([3, 4, 1, 2]));
}

#[test]
fn update_object_element_arrays_concatenate_without_deep_merge() {
    // Object-valued array elements concatenate the same way scalars do. The
    // incoming element comes first, then the existing one. The two objects are
    // not deep-merged with each other.
    let tmp = TempDir::new().unwrap();
    rc9::write(&json!({ "items": [{ "b": 2 }] }), &dir_opts(&tmp)).unwrap();
    let merged = rc9::update(&json!({ "items": [{ "a": 1 }] }), &dir_opts(&tmp)).unwrap();
    assert_eq!(merged["items"], json!([{ "a": 1 }, { "b": 2 }]));
}

#[test]
fn update_null_does_not_clobber() {
    let tmp = TempDir::new().unwrap();
    rc9::write(&json!({ "x": "keep" }), &dir_opts(&tmp)).unwrap();
    let merged = rc9::update(&json!({ "x": null, "y": "added" }), &dir_opts(&tmp)).unwrap();
    assert_eq!(merged["x"], json!("keep"));
    assert_eq!(merged["y"], json!("added"));
}

#[test]
fn update_empty_string_does_clobber() {
    let tmp = TempDir::new().unwrap();
    rc9::write(&json!({ "x": "old" }), &dir_opts(&tmp)).unwrap();
    let merged = rc9::update(&json!({ "x": "" }), &dir_opts(&tmp)).unwrap();
    assert_eq!(merged["x"], json!(""));
}

#[test]
fn update_flat_keeps_literal_keys() {
    let tmp = TempDir::new().unwrap();
    let opts = rc9::RcOptions {
        name: Some(".flat".into()),
        dir: Some(tmp.path().to_string_lossy().into_owned()),
        flat: Some(true),
    };
    let merged = rc9::update(&json!({ "x.y": 2 }), &opts).unwrap();
    assert_eq!(merged, json!({ "x.y": 2 }));
    // The file keeps the literal dotted key.
    let bytes = std::fs::read_to_string(tmp.path().join(".flat")).unwrap();
    assert_eq!(bytes, "x.y=2");
}

#[test]
fn name_shorthand_resolves_in_cwd_relative_dir() {
    // An absolute name overrides the dir, matching path resolution.
    let tmp = TempDir::new().unwrap();
    let abs = tmp.path().join(".abs");
    rc9::write(
        &json!({ "z": 9 }),
        &rc9::RcOptions::name(abs.to_string_lossy().into_owned()),
    )
    .unwrap();
    let read = rc9::read(&rc9::RcOptions::name(abs.to_string_lossy().into_owned()));
    assert_eq!(read, json!({ "z": 9 }));
}

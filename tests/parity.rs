//! Behavioral parity tests mirroring the canonical suite, case for case.

mod common;

use common::{assert_matches_subset, sample_config};
use serde_json::json;
use serial_test::serial;
use std::sync::MutexGuard;
use tempfile::TempDir;

/// Guard that points the user-config helpers at a throwaway directory.
///
/// Sets `XDG_CONFIG_HOME` to a fresh tempdir and clears it on drop. Tests that
/// use it run serially so the process-global env var does not race.
struct UserDir {
    _tmp: TempDir,
    _lock: MutexGuard<'static, ()>,
}

impl UserDir {
    fn new() -> Self {
        let lock = env_lock();
        let tmp = TempDir::new().expect("create tempdir");
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        UserDir {
            _tmp: tmp,
            _lock: lock,
        }
    }
}

impl Drop for UserDir {
    fn drop(&mut self) {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

/// Process-wide lock so env-mutating tests do not overlap.
fn env_lock() -> MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Options pointing at a directory, with the default name.
fn dir_opts(dir: &std::path::Path) -> rc9::RcOptions {
    rc9::RcOptions {
        dir: Some(dir.to_string_lossy().into_owned()),
        ..Default::default()
    }
}

// Case 1: Write config. Round-trip via default name in a temp dir.
#[test]
fn write_config() {
    let tmp = TempDir::new().unwrap();
    let opts = dir_opts(tmp.path());
    rc9::write(&sample_config(), &opts).unwrap();
    assert_matches_subset(&rc9::read(&opts).unwrap(), &sample_config());
}

// Case 2: Write config (user). Round-trip via the user-config directory.
#[test]
#[serial]
fn write_config_user() {
    let _user = UserDir::new();
    rc9::write_user_config(&sample_config(), &rc9::RcOptions::default()).unwrap();
    assert_matches_subset(
        &rc9::read_user_config(&rc9::RcOptions::default()).unwrap(),
        &sample_config(),
    );
}

// Case 3: Read config. String-shorthand options reading a written file.
#[test]
fn read_config() {
    let tmp = TempDir::new().unwrap();
    rc9::write(&sample_config(), &dir_opts(tmp.path())).unwrap();
    // Read it back with name shorthand plus the dir.
    let opts = rc9::RcOptions {
        name: Some(".conf".into()),
        dir: Some(tmp.path().to_string_lossy().into_owned()),
        flat: None,
    };
    assert_matches_subset(&rc9::read(&opts).unwrap(), &sample_config());
}

// Case 4: Update user config. Dotted key unflattens, quoted string round-trips,
// other keys survive the merge.
#[test]
#[serial]
fn update_user_config() {
    let _user = UserDir::new();
    rc9::write_user_config(&sample_config(), &rc9::RcOptions::default()).unwrap();
    rc9::update_user_config(
        &json!({ "db.password": "\"123\"" }),
        &rc9::RcOptions::default(),
    )
    .unwrap();
    let read = rc9::read_user_config(&rc9::RcOptions::default()).unwrap();
    assert_eq!(read["db"]["password"], json!("\"123\""));
    // The merge preserves the other db keys from the existing file.
    assert_eq!(read["db"]["username"], json!("db username"));
    assert_eq!(read["db"]["enabled"], json!(false));
}

// Case 5: Update user config to empty string. The empty value survives.
#[test]
#[serial]
fn update_user_config_to_empty_string() {
    let _user = UserDir::new();
    rc9::write_user_config(&sample_config(), &rc9::RcOptions::default()).unwrap();
    rc9::update_user_config(&json!({ "db.password": "" }), &rc9::RcOptions::default()).unwrap();
    let read = rc9::read_user_config(&rc9::RcOptions::default()).unwrap();
    assert_eq!(read["db"]["password"], json!(""));
}

// Case 6: Write user config (config dir). Custom name via shorthand.
#[test]
#[serial]
fn write_user_config_named() {
    let _user = UserDir::new();
    rc9::write_user_config(&sample_config(), &rc9::RcOptions::name(".conf-user")).unwrap();
    assert_matches_subset(
        &rc9::read_user_config(&rc9::RcOptions::name(".conf-user")).unwrap(),
        &sample_config(),
    );
}

// Case 7: Update user config (config dir). Merge into a named file.
#[test]
#[serial]
fn update_user_config_named() {
    let _user = UserDir::new();
    rc9::write_user_config(&sample_config(), &rc9::RcOptions::name(".conf-user")).unwrap();
    rc9::update_user_config(
        &json!({ "db.password": "updated" }),
        &rc9::RcOptions::name(".conf-user"),
    )
    .unwrap();
    let read = rc9::read_user_config(&rc9::RcOptions::name(".conf-user")).unwrap();
    assert_eq!(read["db"]["password"], json!("updated"));
    assert_eq!(read["db"]["username"], json!("db username"));
}

// Case 8: Parse ignores invalid lines. Comment, no-`=`, proto key, spacing.
#[test]
fn parse_ignore_invalid_lines() {
    let input = "
      foo=bar
      __proto__=no
      # test
      bar = baz
      empty =
    ";
    let parsed = rc9::parse(input, &rc9::RcOptions::default());
    assert_matches_subset(&parsed, &json!({ "foo": "bar", "bar": "baz" }));
    // The proto key is dropped.
    assert!(parsed.get("__proto__").is_none());
}

// Case 9: Ignore non-existent. Missing file yields an empty object.
#[test]
fn ignore_non_existent() {
    let tmp = TempDir::new().unwrap();
    let opts = rc9::RcOptions {
        name: Some(".404".into()),
        dir: Some(tmp.path().to_string_lossy().into_owned()),
        flat: None,
    };
    assert_eq!(rc9::read(&opts).unwrap(), json!({}));
}

// Case 10: Flat mode. Dotted keys are not unflattened and coexist.
#[test]
fn flat_mode() {
    let tmp = TempDir::new().unwrap();
    let object = json!({ "x": 1, "x.y": 2 });
    let opts = rc9::RcOptions {
        name: Some(".conf2".into()),
        dir: Some(tmp.path().to_string_lossy().into_owned()),
        flat: Some(true),
    };
    rc9::update(&object, &opts).unwrap();
    assert_matches_subset(&rc9::read(&opts).unwrap(), &object);
}

// Case 11: Parse indexless arrays. `[]` keys push into a nested array.
#[test]
fn parse_indexless_arrays() {
    let input = "
      x.foo[]=A
      x.foo[]=B
    ";
    let parsed = rc9::parse(input, &rc9::RcOptions::default());
    assert_matches_subset(&parsed, &json!({ "x": { "foo": ["A", "B"] } }));
}

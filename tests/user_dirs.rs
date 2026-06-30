//! User-config directory resolution: XDG, home fallback, and the legacy variants.
//!
//! These mutate process-global env vars, so they run serially and restore the
//! prior values on exit.

use serde_json::json;
use serial_test::serial;
use std::sync::MutexGuard;
use tempfile::TempDir;

/// Saves and restores `XDG_CONFIG_HOME` and `HOME` around a test body.
struct EnvGuard {
    xdg: Option<String>,
    home: Option<String>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn new() -> Self {
        let lock = env_lock();
        EnvGuard {
            xdg: std::env::var("XDG_CONFIG_HOME").ok(),
            home: std::env::var("HOME").ok(),
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        restore("XDG_CONFIG_HOME", &self.xdg);
        restore("HOME", &self.home);
    }
}

fn restore(key: &str, value: &Option<String>) {
    match value {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

fn env_lock() -> MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[test]
#[serial]
fn xdg_config_home_is_honored() {
    let _g = EnvGuard::new();
    let tmp = TempDir::new().unwrap();
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());

    rc9::write_user_config(&json!({ "token": 123 }), &rc9::RcOptions::name(".zoorc")).unwrap();
    // The file lands directly in XDG_CONFIG_HOME.
    let path = tmp.path().join(".zoorc");
    assert!(path.exists());
    assert_eq!(
        rc9::read_user_config(&rc9::RcOptions::name(".zoorc")).unwrap(),
        json!({ "token": 123 })
    );
}

#[test]
#[serial]
fn user_config_falls_back_to_home_config() {
    let _g = EnvGuard::new();
    let home = TempDir::new().unwrap();
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::set_var("HOME", home.path());
    // The fallback target ~/.config must exist for the write to land.
    std::fs::create_dir_all(home.path().join(".config")).unwrap();

    rc9::write_user_config(&json!({ "a": 1 }), &rc9::RcOptions::name(".rc")).unwrap();
    let path = home.path().join(".config").join(".rc");
    assert!(path.exists(), "expected file at ~/.config/.rc");
}

#[test]
#[serial]
fn empty_xdg_falls_back_to_home_config() {
    let _g = EnvGuard::new();
    let home = TempDir::new().unwrap();
    // An empty value is treated as unset.
    std::env::set_var("XDG_CONFIG_HOME", "");
    std::env::set_var("HOME", home.path());
    std::fs::create_dir_all(home.path().join(".config")).unwrap();

    rc9::write_user_config(&json!({ "a": 1 }), &rc9::RcOptions::name(".rc")).unwrap();
    assert!(home.path().join(".config").join(".rc").exists());
}

#[test]
#[serial]
#[allow(deprecated)]
fn legacy_user_writes_to_bare_home() {
    let _g = EnvGuard::new();
    let home = TempDir::new().unwrap();
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::set_var("HOME", home.path());

    rc9::write_user(&json!({ "a": 1 }), &rc9::RcOptions::name(".legacyrc")).unwrap();
    // The legacy path is the bare home directory, not ~/.config.
    assert!(home.path().join(".legacyrc").exists());
    assert!(!home.path().join(".config").join(".legacyrc").exists());
    assert_eq!(
        rc9::read_user(&rc9::RcOptions::name(".legacyrc")).unwrap(),
        json!({ "a": 1 })
    );
}

#[test]
#[serial]
#[allow(deprecated)]
fn legacy_user_honors_xdg() {
    let _g = EnvGuard::new();
    let tmp = TempDir::new().unwrap();
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());

    rc9::write_user(&json!({ "a": 1 }), &rc9::RcOptions::name(".legacyrc")).unwrap();
    assert!(tmp.path().join(".legacyrc").exists());
}

#[test]
#[serial]
#[allow(deprecated)]
fn legacy_update_user_merges() {
    let _g = EnvGuard::new();
    let tmp = TempDir::new().unwrap();
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());

    rc9::write_user(
        &json!({ "db": { "user": "alice", "pass": "old" } }),
        &rc9::RcOptions::name(".u"),
    )
    .unwrap();
    let merged =
        rc9::update_user(&json!({ "db.pass": "new" }), &rc9::RcOptions::name(".u")).unwrap();
    assert_eq!(merged["db"]["pass"], json!("new"));
    assert_eq!(merged["db"]["user"], json!("alice"));
}

//! Read and write flat rc-style dotfile config.
//!
//! A config file is a list of `key=value` lines. Keys may use dot notation
//! (`db.username`) to express nested objects and a `[]` suffix (`modules[]`) to
//! push onto arrays. Values coerce to native types on read and serialize back to
//! JSON-quoted text on write.
//!
//! # Format
//!
//! Each line matches `key=value`. The key is a run of non-whitespace,
//! non-`=` characters. Only the first `=` splits the line, so a value may
//! contain further `=` signs. Lines that do not match, including blanks and
//! comments, are skipped.
//!
//! ```text
//! db.username=username
//! db.password=multi word password
//! db.enabled=true
//! # comment lines are ignored
//! modules[]=test
//! ```
//!
//! # Values
//!
//! Reading coerces values to native types. `count=123` reads as a number.
//! Wrap a value in quotes to keep it a string: `count="123"` reads as the
//! string `123`. See [`parse`].
//!
//! # Nesting
//!
//! Dotted keys nest into objects on read and flatten back on write. Numeric
//! path segments build arrays, so `tags.0=A` reads as `{tags:["A"]}`. Pass
//! `flat: true` to keep literal dotted keys and avoid collisions between keys
//! like `x` and `x.y`.
//!
//! # Example
//!
//! ```
//! use serde_json::json;
//!
//! let config = rc9::parse("db.enabled=true\ncount=3", &rc9::RcOptions::default());
//! assert_eq!(config, json!({ "db": { "enabled": true }, "count": 3 }));
//!
//! let text = rc9::serialize(&json!({ "db": { "enabled": true } }));
//! assert_eq!(text, "db.enabled=true");
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod defu;
mod destr;
mod flat;
mod resolve;

use serde_json::{Map, Value};
use std::path::PathBuf;

/// The configuration file name used when no name is given.
pub const DEFAULT_NAME: &str = ".conf";

/// Options that control where a config file lives and how it parses.
///
/// All fields are optional. A missing field falls back to its default: name
/// `.conf`, directory the current working directory, and nested (not flat) mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RcOptions {
    /// The name of the configuration file.
    pub name: Option<String>,
    /// The directory that holds the configuration file.
    pub dir: Option<String>,
    /// Treat the configuration as a flat map of literal dotted keys.
    pub flat: Option<bool>,
}

impl RcOptions {
    /// Build options from a file name, leaving the other fields at their defaults.
    pub fn name(name: impl Into<String>) -> Self {
        RcOptions {
            name: Some(name.into()),
            ..Default::default()
        }
    }
}

/// Options with every field resolved to a concrete value.
struct Resolved {
    name: String,
    dir: String,
    flat: bool,
}

/// Fill missing option fields with their defaults.
fn with_defaults(options: &RcOptions) -> Resolved {
    Resolved {
        name: options
            .name
            .clone()
            .unwrap_or_else(|| DEFAULT_NAME.to_string()),
        dir: options.dir.clone().unwrap_or_else(default_dir),
        flat: options.flat.unwrap_or(false),
    }
}

/// The default directory: the current working directory as a string.
fn default_dir() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

/// Parse config text into a value tree.
///
/// Splits on line breaks, extracts `key=value` pairs, coerces values to native
/// types, and either nests dotted keys or keeps them flat.
///
/// Skips blank lines, comment lines, lines without `=`, and keys equal to
/// `__proto__`, `constructor`, or the empty string. A key ending in `[]` pushes
/// its value onto an array at the stripped key. With `flat: false` (the
/// default) the result is unflattened into nested objects and arrays.
///
/// # Example
///
/// ```
/// use serde_json::json;
/// let cfg = rc9::parse("a.b=1\nlist[]=x\nlist[]=y", &rc9::RcOptions::default());
/// assert_eq!(cfg, json!({ "a": { "b": 1 }, "list": ["x", "y"] }));
/// ```
pub fn parse(contents: &str, options: &RcOptions) -> Value {
    let mut config = Map::new();

    for line in split_lines(contents) {
        let Some((key, raw_value)) = match_key_val(line) else {
            continue;
        };
        if key.is_empty() || key == "__proto__" || key == "constructor" {
            continue;
        }

        let value = destr::destr(raw_value.trim_matches(destr::js_whitespace));

        if let Some(nkey) = key.strip_suffix("[]") {
            // Update in place so re-touching the key keeps its original position.
            // push_concat is always Some for an existing value, so the slot never
            // keeps the null left by take.
            if let Some(slot) = config.get_mut(nkey) {
                if let Some(merged) = push_concat(Some(slot.take()), value) {
                    *slot = merged;
                }
            } else if let Some(merged) = push_concat(None, value) {
                config.insert(nkey.to_string(), merged);
            }
            continue;
        }

        config.insert(key.to_string(), value);
    }

    let config = Value::Object(config);
    if options.flat.unwrap_or(false) {
        config
    } else {
        flat::unflatten(&config)
    }
}

/// Apply the array-push rule `(existing || []).concat(value)` for a `[]` key.
///
/// `existing` is the current value at the stripped key, if any. The rule treats
/// a falsy existing value as an empty array, so it starts fresh and the pushed
/// value forms the first elements. A non-empty array extends. A non-empty string
/// concatenates the pushed value's string form.
///
/// `concat` spreads an array argument and appends a scalar as one element.
///
/// A non-empty number, a `true`, or an object has no `concat` method, so the
/// canonical form raises a `TypeError`. This function has no way to fail, so it
/// returns `None` to leave the existing value in place. That keeps the parser
/// total and matches the shape of "this push does not apply here".
fn push_concat(existing: Option<Value>, value: Value) -> Option<Value> {
    match existing {
        None => Some(concat_onto_array(Vec::new(), value)),
        Some(current) if is_js_falsy(&current) => Some(concat_onto_array(Vec::new(), value)),
        Some(Value::Array(arr)) => Some(concat_onto_array(arr, value)),
        Some(Value::String(s)) => Some(Value::String(s + &js_string(&value))),
        Some(other) => Some(other),
    }
}

/// Build an array from `base`, appending `value` the way `Array.concat` does.
fn concat_onto_array(mut base: Vec<Value>, value: Value) -> Value {
    match value {
        Value::Array(items) => base.extend(items),
        scalar => base.push(scalar),
    }
    Value::Array(base)
}

/// Whether a value is falsy by JS rules: `null`, `false`, `0`, or `""`.
fn is_js_falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

/// Render a value as JS string concatenation would, for the string-push case.
fn js_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => encode_value(&Value::Number(n.clone())),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Split text into lines on `\n`, `\r\n`, or `\r`.
fn split_lines(contents: &str) -> impl Iterator<Item = &str> {
    contents.split(['\n', '\r'])
}

/// Match a line against the key-value grammar.
///
/// Returns the key and the raw value with leading whitespace already stripped.
/// The grammar is `^\s*([^\s=]+)\s*=\s*(.*)?\s*$`. The key is the first run of
/// non-whitespace, non-`=` characters. Everything after the first `=` (past the
/// optional surrounding whitespace) is the value.
fn match_key_val(line: &str) -> Option<(&str, &str)> {
    // Leading whitespace before the key.
    let after_lead = line.trim_start_matches(destr::js_whitespace);

    // Key: one or more chars that are not whitespace and not `=`.
    let key_end = after_lead
        .find(|c: char| destr::js_whitespace(c) || c == '=')
        .unwrap_or(after_lead.len());
    if key_end == 0 {
        return None;
    }
    let key = &after_lead[..key_end];

    // Optional whitespace, then a required `=`, then optional whitespace.
    let rest = after_lead[key_end..].trim_start_matches(destr::js_whitespace);
    let value_part = rest.strip_prefix('=')?;
    let value = value_part.trim_start_matches(destr::js_whitespace);

    Some((key, value))
}

/// Parse a config file at `path`, returning an empty object if it does not exist.
///
/// Reads the file as UTF-8 and delegates to [`parse`]. A missing file yields an
/// empty object with no error. Any other failure, such as a permission error, a
/// directory in place of a file, or invalid UTF-8, returns `Err` rather than
/// looking the same as an absent file.
pub fn parse_file(path: &std::path::Path, options: &RcOptions) -> std::io::Result<Value> {
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(parse(&contents, options)),
        // The file vanished between the existence check and the read. Treat the
        // race the same as a missing file.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(err) => Err(err),
    }
}

/// Read and parse a config file from the configured directory and name.
///
/// Resolves the path from `dir` and `name`, then parses it. A missing file
/// yields an empty object. Other read failures return `Err`.
pub fn read(options: &RcOptions) -> std::io::Result<Value> {
    let resolved = with_defaults(options);
    let path = resolve::resolve(&resolved.dir, &resolved.name);
    parse_file(&path, options)
}

/// Read a user config from `$XDG_CONFIG_HOME` or the bare home directory.
///
/// Deprecated. Prefer [`read_user_config`], which follows the XDG convention and
/// uses `~/.config`.
#[deprecated(note = "use read_user_config, which uses ~/.config following XDG conventions")]
pub fn read_user(options: &RcOptions) -> std::io::Result<Value> {
    let mut opts = options.clone();
    opts.dir = Some(legacy_user_dir());
    read(&opts)
}

/// Read a user config from `$XDG_CONFIG_HOME` or `~/.config`.
pub fn read_user_config(options: &RcOptions) -> std::io::Result<Value> {
    let mut opts = options.clone();
    opts.dir = Some(xdg_config_dir());
    read(&opts)
}

/// Serialize a config value into rc-format text.
///
/// Flattens the value into dotted keys, then emits each entry as
/// `key=<json>` where the value is JSON-encoded. Strings are quoted, numbers and
/// booleans are bare, and `null` writes as `null`. Lines join with `\n` and
/// there is no trailing newline.
///
/// # Example
///
/// ```
/// use serde_json::json;
/// let text = rc9::serialize(&json!({ "db": { "user": "alice", "on": true } }));
/// assert_eq!(text, "db.user=\"alice\"\ndb.on=true");
/// ```
pub fn serialize(config: &Value) -> String {
    flat::flatten(config)
        .iter()
        .map(|(key, value)| format!("{key}={}", encode_value(value)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// JSON-encode a leaf value the way `JSON.stringify` would for a single value.
///
/// Non-finite numbers and null encode as `null`. serde_json already handles
/// these. The serde_json default escaping matches the source: it escapes the
/// mandatory characters and leaves forward slashes and non-ASCII untouched.
fn encode_value(value: &Value) -> String {
    // An integral float prints without a decimal point, matching a single
    // number type. serde_json would otherwise print `1000.0`.
    if let Value::Number(num) = value {
        if !num.is_i64() && !num.is_u64() {
            if let Some(f) = num.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e21 {
                    // Negative zero prints as a bare 0. `-0.0 == 0.0` holds in
                    // IEEE-754, so this equality test catches both signs and
                    // drops the sign byte that `{:.0}` would emit for -0.0.
                    if f == 0.0 {
                        return "0".to_string();
                    }
                    return format!("{f:.0}");
                }
            }
        }
    }
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// Write a config value to the configured file as UTF-8.
///
/// Resolves the path, serializes the value, and writes it, creating or
/// truncating the file. This ignores `flat` and always flattens.
pub fn write(config: &Value, options: &RcOptions) -> std::io::Result<()> {
    let resolved = with_defaults(options);
    let path = resolve::resolve(&resolved.dir, &resolved.name);
    std::fs::write(path, serialize(config))
}

/// Write a user config to `$XDG_CONFIG_HOME` or the bare home directory.
///
/// Deprecated. Prefer [`write_user_config`].
#[deprecated(note = "use write_user_config, which uses ~/.config following XDG conventions")]
pub fn write_user(config: &Value, options: &RcOptions) -> std::io::Result<()> {
    let mut opts = options.clone();
    opts.dir = Some(legacy_user_dir());
    write(config, &opts)
}

/// Write a user config to `$XDG_CONFIG_HOME` or `~/.config`.
pub fn write_user_config(config: &Value, options: &RcOptions) -> std::io::Result<()> {
    let mut opts = options.clone();
    opts.dir = Some(xdg_config_dir());
    write(config, &opts)
}

/// Merge a config into the file on disk and write the result.
///
/// Reads the existing config, merges the incoming value over it (incoming wins,
/// arrays concatenate incoming-first, nested objects deep-merge, `null` is
/// skipped), writes the merged value, and returns it. With `flat: false` the
/// incoming value is unflattened first, so callers may pass dotted keys.
///
/// # Example
///
/// ```no_run
/// use serde_json::json;
/// let merged = rc9::update(&json!({ "db.user": "alice" }), &rc9::RcOptions::name(".myrc"))?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn update(config: &Value, options: &RcOptions) -> std::io::Result<Value> {
    let resolved = with_defaults(options);

    let incoming = if resolved.flat {
        config.clone()
    } else {
        flat::unflatten(config)
    };

    let existing = read(options)?;
    let new_config = defu::defu(&incoming, &existing);
    write(&new_config, options)?;
    Ok(new_config)
}

/// Update a user config in `$XDG_CONFIG_HOME` or the bare home directory.
///
/// Deprecated. Prefer [`update_user_config`].
#[deprecated(note = "use update_user_config, which uses ~/.config following XDG conventions")]
pub fn update_user(config: &Value, options: &RcOptions) -> std::io::Result<Value> {
    let mut opts = options.clone();
    opts.dir = Some(legacy_user_dir());
    update(config, &opts)
}

/// Update a user config in `$XDG_CONFIG_HOME` or `~/.config`.
pub fn update_user_config(config: &Value, options: &RcOptions) -> std::io::Result<Value> {
    let mut opts = options.clone();
    opts.dir = Some(xdg_config_dir());
    update(config, &opts)
}

/// The user-config directory: `$XDG_CONFIG_HOME` if set and non-empty, else `~/.config`.
fn xdg_config_dir() -> String {
    if let Some(xdg) = non_empty_env("XDG_CONFIG_HOME") {
        return xdg;
    }
    let home = home_dir();
    resolve::resolve_from(&home, ".config")
        .to_string_lossy()
        .into_owned()
}

/// The legacy user-config directory: `$XDG_CONFIG_HOME` if set and non-empty, else the home directory.
fn legacy_user_dir() -> String {
    if let Some(xdg) = non_empty_env("XDG_CONFIG_HOME") {
        return xdg;
    }
    home_dir().to_string_lossy().into_owned()
}

/// Read an env var, treating an empty value as unset.
fn non_empty_env(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// The user's home directory, or `.` if it cannot be found.
fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

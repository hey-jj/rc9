# rc9

Read and write flat rc-style dotfile config.

A config file is a list of `key=value` lines. Keys may use dot notation
(`db.username`) for nested objects and a `[]` suffix (`modules[]`) to push onto
arrays. Values coerce to native types on read and serialize back to JSON-quoted
text on write.

```ini
db.username=username
db.password=multi word password
db.enabled=true
# comment lines are ignored
modules[]=test
```

## Installation

```toml
[dependencies]
rc9 = "0.1"
```

## Usage

Values are modeled as `serde_json::Value`. The crate re-uses its `json!` macro
in examples.

```rust
use serde_json::json;

// Parse text into a nested value tree.
let cfg = rc9::parse("db.enabled=true\ncount=3", &rc9::RcOptions::default());
assert_eq!(cfg, json!({ "db": { "enabled": true }, "count": 3 }));

// Serialize a value back to rc text. Strings are quoted, numbers are bare.
let text = rc9::serialize(&json!({ "db": { "user": "alice" } }));
assert_eq!(text, "db.user=\"alice\"");
```

Read and write a file by directory and name:

```rust,no_run
let opts = rc9::RcOptions::name(".myrc");
let config = rc9::read(&opts)?;
rc9::write(&config, &opts)?;
# Ok::<(), std::io::Error>(())
```

Merge an update into the file on disk and get the result back:

```rust,no_run
use serde_json::json;
let merged = rc9::update(&json!({ "db.user": "alice" }), &rc9::RcOptions::name(".myrc"))?;
# Ok::<(), std::io::Error>(())
```

## Values

Reading coerces values to native types. `count=123` reads as a number. Wrap a
value in quotes to keep it a string: `count="123"` reads as the string `123`.
Booleans, `null`, and JSON objects and arrays are recognized. Anything else
stays a string.

## Nesting

Dotted keys nest into objects on read and flatten back on write. Numeric path
segments build arrays, so `tags.0=A` reads as `{tags:["A"]}`. A key ending in
`[]` pushes onto an array. Pass `flat: true` to keep literal dotted keys and
avoid collisions between keys like `x` and `x.y`.

## User config

`read_user_config`, `write_user_config`, and `update_user_config` store config in
the user config directory, which is `$XDG_CONFIG_HOME` or `~/.config`.

```rust,no_run
use serde_json::json;
rc9::write_user_config(&json!({ "token": 123 }), &rc9::RcOptions::name(".zoorc"))?;
let conf = rc9::read_user_config(&rc9::RcOptions::name(".zoorc"))?;
# Ok::<(), std::io::Error>(())
```

The `read_user`, `write_user`, and `update_user` functions are deprecated. They
use the bare home directory instead of `~/.config`.

## License

Licensed under the [MIT license](LICENSE). See the LICENSE file for the bundled
attribution notice.

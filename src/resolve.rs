//! Path resolution that mirrors joining two path segments into an absolute path.
//!
//! Walks right to left. The first absolute segment wins. A relative result is
//! prefixed with the current working directory. The path is normalized without
//! touching the filesystem, so `.` and `..` are resolved textually and symlinks
//! are left alone.

use std::path::{Component, Path, PathBuf};

/// Resolve `name` against `dir` into an absolute, normalized path.
///
/// If `name` is absolute it is used directly. Otherwise it is joined onto `dir`.
/// If the result is still relative it is joined onto the current working
/// directory. The final path is normalized textually.
pub fn resolve(dir: &str, name: &str) -> PathBuf {
    let name_path = Path::new(name);
    if name_path.is_absolute() {
        return normalize(name_path);
    }

    let dir_path = Path::new(dir);
    let joined = if dir_path.is_absolute() {
        dir_path.join(name_path)
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        cwd.join(dir_path).join(name_path)
    };
    normalize(&joined)
}

/// Resolve `parts` onto the home-style base directory, right to left.
///
/// Used by the user-config helpers to build `<base>/<segment>` while honoring an
/// absolute segment that should override the base.
pub fn resolve_from(base: &Path, segment: &str) -> PathBuf {
    let seg = Path::new(segment);
    if seg.is_absolute() {
        normalize(seg)
    } else {
        normalize(&base.join(seg))
    }
}

/// Collapse `.` and `..` components without consulting the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

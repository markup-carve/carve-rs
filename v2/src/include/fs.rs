//! The file resolver of a command-line tool (C2-N-012 to C2-N-014): paths
//! resolve against the including file, and only files whose canonical path
//! lies inside the canonical root are read.

use super::{Cause, Request, Resolver};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Why a root is refused (C2-N-014).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootError {
    NotAbsolute,
    Unreadable,
}

/// A resolver confined to `root`, an absolute path. An id is the canonical
/// path of the file relative to the root, with `/` between its parts.
///
/// # Errors
///
/// [`RootError`] for a relative root or one that cannot be canonicalized.
pub fn resolver(root: &Path) -> Result<Resolver, RootError> {
    if !root.is_absolute() {
        return Err(RootError::NotAbsolute);
    }
    let root = std::fs::canonicalize(root).map_err(|_| RootError::Unreadable)?;
    Ok(Arc::new(move |r: &Request<'_>| read(&root, r)))
}

/// C2-N-012: a path that starts with a URI scheme, letters and `:` before any `/`.
fn has_scheme(path: &str) -> bool {
    let head = path.split('/').next().unwrap_or("");
    head.find(':').is_some_and(|colon| {
        colon > 0
            && head.get(..colon).is_some_and(|s| {
                s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
            })
            && head.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
    })
}

/// C2-N-013: the canonical form of `path`: its longest existing prefix
/// canonicalized, the rest appended with `.` and `..` read lexically.
fn canonical(path: &Path) -> Option<PathBuf> {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    while std::fs::symlink_metadata(&existing).is_err() {
        let name = existing.file_name()?.to_os_string();
        rest.push(name);
        if !existing.pop() {
            return None;
        }
    }
    let mut out = std::fs::canonicalize(&existing).ok()?;
    for part in rest.iter().rev() {
        match Path::new(part).components().next() {
            Some(Component::ParentDir) => {
                out.pop();
            }
            Some(Component::CurDir) | None => {}
            Some(_) => out.push(part),
        }
    }
    Some(out)
}

/// Lexical `.` and `..` of the joined path, so a missing directory before a
/// `..` does not hide where the path leads.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn read(root: &Path, r: &Request<'_>) -> Result<(Vec<u8>, String), Cause> {
    if has_scheme(r.path) || r.path.is_empty() {
        return Err(Cause::OutsideRoot);
    }
    let base = r.from.map_or_else(
        || root.to_path_buf(),
        |f| {
            root.join(f)
                .parent()
                .map_or_else(|| root.to_path_buf(), Path::to_path_buf)
        },
    );
    let joined = lexical(&base.join(r.path));
    let target = canonical(&joined).ok_or(Cause::OutsideRoot)?;
    if !target.starts_with(root) {
        return Err(Cause::OutsideRoot);
    }
    if std::fs::symlink_metadata(&target).is_err() {
        return Err(Cause::NotFound);
    }
    let file = std::fs::File::open(&target).map_err(|_| Cause::Unreadable)?;
    if !opened_inside(&file, &target, root) {
        return Err(Cause::OutsideRoot);
    }
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err(Cause::Unreadable);
    }
    let mut bytes = Vec::new();
    let cap = u64::try_from(crate::tables::INCLUDE_BYTES).unwrap_or(u64::MAX) + 1;
    // C2-N-065: past the cap the expansion refuses the text, so reading more is no use.
    file.take(cap)
        .read_to_end(&mut bytes)
        .map_err(|_| Cause::Unreadable)?;
    let id = target
        .strip_prefix(root)
        .map_err(|_| Cause::OutsideRoot)?
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Ok((bytes, id))
}

/// C2-N-013: the file that was opened is the file that was tested. On Linux
/// the kernel names the open file; elsewhere the identity of the handle must
/// equal that of the tested path.
#[cfg(target_os = "linux")]
fn opened_inside(file: &std::fs::File, _target: &Path, root: &Path) -> bool {
    use std::os::fd::AsRawFd;
    std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
        .is_ok_and(|p| p.starts_with(root))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn opened_inside(file: &std::fs::File, target: &Path, _root: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (file.metadata(), std::fs::metadata(target)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn opened_inside(_file: &std::fs::File, _target: &Path, _root: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn c2_n_012_uri_schemes() {
        assert!(super::has_scheme("https://x.org/a.crv"));
        assert!(super::has_scheme("file:a.crv"));
        assert!(!super::has_scheme("a/b:c.crv"));
        assert!(!super::has_scheme(":a"));
    }
}

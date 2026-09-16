//! PART 9 §19 I11: an unresolved target's id is resolved against the including
//! file like a resolved one, and a path escaping the containment root keeps the
//! directive's spelling.
//!
//! The spec suite pins both halves as
//! `i11-fs-missing-target-below-the-root-names-where-it-would-appear` and
//! `i11-fs-escaping-target-below-the-root-keeps-its-spelling`. This is the same
//! pair against a real tree, so the behavior is gated whatever the spec pin is
//! at.

#![cfg(feature = "fs")]

use std::fs;
use std::path::{Path, PathBuf};

use carve::{expand_includes, parse, FileSystemResolver, IncludeOptions};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let mut base = std::env::temp_dir();
        base.push(format!(
            "carve-i11-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&base).expect("temp dir");
        Self(fs::canonicalize(&base).expect("canonical temp dir"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, contents: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("parent dir");
        }
        fs::write(&full, contents).expect("write");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ENTRY: &str = "{{ sub/frag.crv }}\n";

/// The dependency set with the tree base folded to `<TMP>`, matching how the
/// conformance vectors spell a filesystem id.
fn dependencies(tmp: &TempDir, root: &Path) -> Vec<(String, bool)> {
    let resolver = FileSystemResolver::new(root).expect("root exists");
    let result = expand_includes(
        parse(ENTRY),
        ENTRY,
        &IncludeOptions::new().with_resolver(&resolver),
    );
    let base = tmp.path().to_string_lossy().into_owned();
    result
        .dependencies
        .iter()
        .map(|d| {
            let id = match d.id.strip_prefix(&format!("{base}/")) {
                Some(rest) => format!("<TMP>/{rest}"),
                None => d.id.clone(),
            };
            (id, d.resolved)
        })
        .collect()
}

#[test]
fn names_a_missing_target_below_the_root_by_where_it_would_appear() {
    let tmp = TempDir::new("missing");
    tmp.write("main.crv", ENTRY);
    tmp.write("sub/frag.crv", "{{ missing.crv }}\n");

    assert_eq!(
        dependencies(&tmp, tmp.path()),
        vec![
            ("<TMP>/sub/frag.crv".to_string(), true),
            ("<TMP>/sub/missing.crv".to_string(), false),
        ]
    );
}

/// The scope limit. Resolving the id against the including file must not turn
/// an out-of-root refusal into one that reads as if it were inside.
#[test]
fn keeps_the_directive_spelling_for_a_target_that_escapes_the_root() {
    let tmp = TempDir::new("escaping");
    tmp.write("root/main.crv", ENTRY);
    tmp.write("root/sub/frag.crv", "{{ ../../secret.crv }}\n");
    tmp.write("secret.crv", "TOP SECRET\n");

    assert_eq!(
        dependencies(&tmp, &tmp.path().join("root")),
        vec![
            ("<TMP>/root/sub/frag.crv".to_string(), true),
            ("../../secret.crv".to_string(), false),
        ]
    );
}

/// A target that is not there but would land outside the root is refused the
/// same way.
#[test]
fn keeps_the_directive_spelling_for_a_missing_target_outside_the_root() {
    let tmp = TempDir::new("missing-outside");
    tmp.write("root/main.crv", ENTRY);
    tmp.write("root/sub/frag.crv", "{{ ../../gone.crv }}\n");

    assert_eq!(
        dependencies(&tmp, &tmp.path().join("root")),
        vec![
            ("<TMP>/root/sub/frag.crv".to_string(), true),
            ("../../gone.crv".to_string(), false),
        ]
    );
}

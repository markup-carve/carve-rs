//! The `fs` feature is the whole guarantee, so the guarantee gets a gate.
//!
//! `--no-default-features` is supposed to leave a build with NO code that opens
//! a file - the property carve-js gets structurally, by keeping its filesystem
//! resolver behind a `./node` subpath so the browser bundle cannot contain it.
//! Here it is a `#[cfg]`, and a `#[cfg]` is only as good as the next person
//! remembering it: one ungated `std::fs::read_to_string` added anywhere in the
//! library silently takes the property away, and every existing test stays
//! green because they all run WITH the feature.
//!
//! So the library's filesystem call sites are an inventory. Adding one is fine;
//! adding one without deciding which side of the feature it belongs on is not.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every shipped library file. `main.rs` is excluded on purpose: the CLI reads
/// the document named on its own command line, which is its job and has nothing
/// to do with inclusion.
fn library_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("src is readable") {
            let path = entry.expect("entry is readable").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && path.file_name().and_then(|n| n.to_str()) != Some("main.rs")
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut out,
    );
    out.sort();
    out
}

/// A line that reaches the filesystem. Deliberately a wide net - `fs::` catches
/// the imported spelling as well as the fully qualified one.
fn opens_a_file(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or(line);
    code.contains("fs::") || code.contains("File::open") || code.contains("File::create")
}

/// The shipped lines of a file: everything outside a `#[cfg(test)]` module.
///
/// Test modules are skipped STRUCTURALLY rather than listed, because what the
/// feature promises is about a shipped build - a corpus reader in a unit test
/// is not in one at all. Listing them instead would have put three entries in
/// the inventory below that say nothing about the guarantee.
///
/// The shape relied on is this crate's own: `#[cfg(test)]` then `mod ... {` at
/// column zero, closed by a `}` at column zero. A test module written some
/// other way would be scanned rather than skipped, which fails loudly and is
/// the safe direction for this gate to be wrong in.
fn shipped_lines(source: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_test_module = false;
    let mut pending_test_attr = false;
    for line in source.lines() {
        if in_test_module {
            if line == "}" {
                in_test_module = false;
            }
            continue;
        }
        if line.trim() == "#[cfg(test)]" {
            pending_test_attr = true;
            continue;
        }
        if pending_test_attr {
            pending_test_attr = false;
            if line.starts_with("mod ") || line.starts_with("pub mod ") {
                in_test_module = true;
                continue;
            }
        }
        out.push(line);
    }
    out
}

#[test]
fn every_library_filesystem_call_is_accounted_for() {
    // file -> the call sites it is allowed to hold, by the code on the line.
    // ONE entry per call, so a second call on a known line is still a finding.
    let allowed: BTreeSet<(&str, &str)> = [
        // The filesystem resolver, the only shipped reader, gated on `fs`.
        ("includes.rs", "root_real: std::fs::canonicalize(root)?,"),
        (
            "includes.rs",
            "let real = std::fs::canonicalize(&candidate).ok()?;",
        ),
        (
            "includes.rs",
            "if std::fs::metadata(&real).ok()?.len() > limit {",
        ),
        (
            "includes.rs",
            "let source = std::fs::read_to_string(&real).ok()?;",
        ),
    ]
    .into_iter()
    .collect();

    let mut found: BTreeSet<(String, String)> = BTreeSet::new();
    for path in library_sources() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).expect("source is readable");
        for line in shipped_lines(&source) {
            if opens_a_file(line) {
                found.insert((name.clone(), line.trim().to_string()));
            }
        }
    }

    let allowed_owned: BTreeSet<(String, String)> = allowed
        .iter()
        .map(|(f, l)| ((*f).to_string(), (*l).to_string()))
        .collect();

    let added: Vec<_> = found.difference(&allowed_owned).collect();
    assert!(
        added.is_empty(),
        "a library filesystem call is not accounted for. Either gate it on the \
         `fs` feature (the crate promises `--no-default-features` opens no file) \
         or add it to this inventory with the reason it is safe:\n{added:#?}"
    );

    // The other direction: an entry matching nothing is an excuse with no
    // expiry, and would let the real call it described be deleted or moved
    // without anyone noticing this gate stopped covering it.
    let stale: Vec<_> = allowed_owned.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "this inventory names a call site that no longer exists; delete the entry:\n{stale:#?}"
    );
}

#[test]
fn the_resolver_is_gated_on_the_feature() {
    // The inventory above says WHERE the calls are; this says the block holding
    // them actually carries the attribute. Without it every call would still be
    // "accounted for" while compiling into a no-default-features build.
    let source =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/includes.rs"))
            .expect("includes.rs is readable");

    for item in [
        "pub struct FileSystemResolver {",
        "impl FileSystemResolver {",
        "impl IncludeResolver for FileSystemResolver {",
        "pub const DEFAULT_MAX_FILE_BYTES: u64",
    ] {
        let at = source
            .find(item)
            .unwrap_or_else(|| panic!("{item} not found"));
        let preceding = &source[..at];
        let line_before = preceding.lines().last().unwrap_or_default().trim();
        assert_eq!(
            line_before, "#[cfg(feature = \"fs\")]",
            "{item} is not gated on the `fs` feature"
        );
    }
}

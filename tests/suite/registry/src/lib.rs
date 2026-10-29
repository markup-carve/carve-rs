//! `suite_modules!()` declares one `#[path] mod` per `tests/*.rs` at the call
//! site, so a new test file needs no registration line.
//!
//! Skipped: files Cargo.toml declares as their own `[[test]]` target, and files
//! the calling `tests/suite/main.rs` still registers by hand.
//!
//! rustc does not see the directory listing, so a file added after the last
//! build stays out until the suite recompiles. The suite's registry test
//! compares `GENERATED_TEST_FILES` with the disk and catches that.

use proc_macro::TokenStream;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[proc_macro]
pub fn suite_modules(input: TokenStream) -> TokenStream {
    assert!(input.is_empty(), "suite_modules!() takes no arguments");
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let root = Path::new(&root);
    let tests = root.join("tests");

    let mut skipped = BTreeSet::new();
    skipped.extend(registered(&root.join("Cargo.toml"), "path = \"tests/"));
    skipped.extend(registered(&tests.join("suite/main.rs"), "#[path = \"../"));

    let files: BTreeSet<String> = fs::read_dir(&tests)
        .unwrap_or_else(|error| panic!("cannot list {}: {error}", tests.display()))
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".rs") && !skipped.contains(name))
        .collect();

    let mut out = String::new();
    for name in &files {
        let path = tests.join(name);
        let path = path.to_str().expect("test paths are UTF-8");
        out.push_str(&format!(
            "#[path = {path:?}] mod {};\n",
            name.trim_end_matches(".rs")
        ));
    }
    out.push_str("/// The files `suite_modules!()` declared, sorted.\n");
    out.push_str("#[allow(dead_code)] const GENERATED_TEST_FILES: &[&str] = &[");
    for name in &files {
        out.push_str(&format!("{name:?},"));
    }
    out.push_str("];\n");
    out.parse().expect("generated module list is valid Rust")
}

/// File names after `prefix` on lines of `file`, for entries directly in `tests/`.
fn registered(file: &Path, prefix: &str) -> Vec<String> {
    let text = fs::read_to_string(file)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", file.display()));
    text.lines()
        .filter_map(|line| {
            let name = line.trim().strip_prefix(prefix)?.split('"').next()?;
            (!name.contains('/')).then(|| name.to_string())
        })
        .collect()
}

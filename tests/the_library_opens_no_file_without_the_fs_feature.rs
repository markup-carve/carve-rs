//! `--no-default-features` promises a library with no code that opens a file.
//! A `#[cfg]` is only as good as the next person remembering it, so the
//! library's filesystem call sites are an inventory: a new one has to be put on
//! one side of the `fs` feature on purpose.

use std::path::{Path, PathBuf};

fn src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every library file. `main.rs` and `lint_cli.rs` belong to the binary, which
/// requires `fs`; `include/fs.rs` is the resolver module, gated as a whole.
fn library_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("src is readable") {
            let path = entry.expect("entry is readable").path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && ![
                    root.join("main.rs"),
                    root.join("lint_cli.rs"),
                    root.join("include/fs.rs"),
                ]
                .contains(&path)
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    let root = src();
    walk(&root, &root, &mut out);
    out.sort();
    out
}

fn opens_a_file(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or(line);
    code.match_indices("fs::").any(|(at, _)| {
        at == 0
            || !matches!(code.as_bytes()[at - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
    }) || code.contains("File::open")
        || code.contains("File::create")
}

fn line_before<'a>(source: &'a str, item: &str) -> &'a str {
    let at = source
        .find(item)
        .unwrap_or_else(|| panic!("{item} not found"));
    source[..at].lines().last().unwrap_or_default().trim()
}

#[test]
fn every_library_filesystem_call_is_accounted_for() {
    let allowed = [(
        "options.rs",
        "crate::include::fs::resolver(std::path::Path::new(root))",
    )];
    let mut found = Vec::new();
    for path in library_sources() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).expect("source is readable");
        for line in source.lines().filter(|line| opens_a_file(line)) {
            found.push((name.clone(), line.trim().to_string()));
        }
    }
    let unexpected: Vec<_> = found
        .iter()
        .filter(|(f, l)| !allowed.iter().any(|(af, al)| af == f && al == l))
        .collect();
    assert!(
        unexpected.is_empty(),
        "gate this filesystem call on the `fs` feature or add it here with the reason:\n{unexpected:#?}"
    );
    for (f, l) in allowed {
        assert!(
            found.iter().any(|(ff, ll)| ff == f && ll == l),
            "stale entry, the call no longer exists: {f}: {l}"
        );
    }
}

#[test]
fn the_filesystem_paths_are_gated_on_the_feature() {
    let include = std::fs::read_to_string(src().join("include/mod.rs")).unwrap();
    assert_eq!(
        line_before(&include, "pub mod fs;"),
        "#[cfg(feature = \"fs\")]"
    );
    let options = std::fs::read_to_string(src().join("api/options.rs")).unwrap();
    assert_eq!(
        line_before(&options, "fn root_resolver(root: &str)"),
        "#[cfg(feature = \"fs\")]"
    );
}

//! Display math that is a list item's marker-line content takes the caption
//! line in its slot (PART 9 §4), the way an image or a block quote there does.
//!
//! Each `NN.html` under `tests/fixtures/marker-line-equation-caption/` is the
//! executable spec's output for `NN.crv` (`scripts/spec` at markup-carve/carve
//! a61aa4d). Rows 11 and up are controls that already matched.

use carve::{to_html, to_html_with_options, Options};
use std::path::Path;

#[test]
#[ignore = "rule A ledger: carve-rs#3523"]
fn the_fixtures_match_the_executable_spec() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/marker-line-equation-caption");
    let mut seen = 0;
    let mut failed = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("crv") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("html")).unwrap();
        let expected = expected.strip_suffix('\n').unwrap();
        let plain = to_html(&source);
        let positioned = to_html_with_options(&source, &Options::default().with_positions(true));
        if plain != expected || positioned != expected {
            failed.push(format!("{}:\n{plain}\n{positioned}", path.display()));
        }
        seen += 1;
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
    assert_eq!(seen, 18);
}

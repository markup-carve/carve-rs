//! A line in the band between a list's base and an item's content column
//! continues a paragraph only where one is open at the deepest frame. Otherwise
//! it closes the list and begins a new block, and no surviving frame adopts it
//! (CARVE-P0-009).
//!
//! Each `NN.html` under `tests/fixtures/band-line-with-no-open-paragraph/` is
//! the executable spec's output for `NN.crv` (`scripts/spec` at
//! markup-carve/carve a61aa4d). Rows 29 and up are controls that already
//! matched; 40 and 41 keep a lone `+` in an open container paragraph.
//! Row 32 follows carve `ba7439f`: a below-column comment under a marker-line
//! container with no open paragraph ends the item (corpus 556-13).

use carve::{to_html, to_html_with_options, Options};
use std::path::Path;

#[test]
fn the_fixtures_match_the_executable_spec() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/band-line-with-no-open-paragraph");
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
    assert_eq!(seen, 41);
}

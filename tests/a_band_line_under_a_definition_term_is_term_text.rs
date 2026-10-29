//! A block-shaped line in the band between a list's base and the item's
//! content column, under a definition term or description, is retained text
//! (PART 1 S4: "A retained list marker below the item's content column remains
//! paragraph text"). The lazy fold kept one column on such a line to say so,
//! which a list marker ignores: a term ends at a marker at any indent.
//!
//! Each `NN.html` under `tests/fixtures/band-line-under-term/` is the
//! executable spec's output for `NN.crv` (`scripts/spec` at markup-carve/carve
//! 77e4ce1). The last rows are controls that already matched.

use carve::{to_html, to_html_with_options, Options};
use std::path::Path;

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn the_fixtures_match_the_executable_spec() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/band-line-under-term");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("crv") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("html")).unwrap();
        let expected = expected.strip_suffix('\n').unwrap();
        assert_eq!(to_html(&source), expected, "{}", path.display());
        assert_eq!(
            to_html_with_options(&source, &Options::default().with_positions(true)),
            expected,
            "positions: {}",
            path.display()
        );
        seen += 1;
    }
    assert_eq!(seen, 28);
}

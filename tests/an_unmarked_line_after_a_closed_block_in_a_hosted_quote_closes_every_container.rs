//! An unmarked line after a block that leaves no open paragraph in a quote an
//! item hosts has no lazy branch (PART 1 S4), so it closes every container.
//!
//! Each `NN.html` under
//! `tests/fixtures/unmarked-line-after-a-closed-block-in-a-hosted-quote/` is the
//! executable spec's output for `NN.crv` (`scripts/spec` at markup-carve/carve
//! 233a6c72). Rows 16 to 18 are controls where a container keeps the line.

use carve::{to_html, to_html_with_options, Options};
use std::path::Path;

#[test]
fn the_fixtures_match_the_executable_spec() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/unmarked-line-after-a-closed-block-in-a-hosted-quote");
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
    assert_eq!(seen, 20);
}

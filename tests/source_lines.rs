//! Opt-in `data-source-line` stamping for editor preview scroll-sync.

use carve::{to_html, to_html_with_options, Options};

#[test]
fn source_lines_disabled_by_default() {
    let html = to_html("# Heading\n\nPara one.\n");
    assert!(!html.contains("data-source-line"), "got: {html}");
}

#[test]
fn source_lines_stamps_top_level_blocks_one_based() {
    let opts = Options::new().with_source_lines(true);
    // 1-based source lines: 1 "# Heading", 3 "Para one.", 5 "Para two."
    let html = to_html_with_options("# Heading\n\nPara one.\n\nPara two.\n", &opts);
    assert!(html.contains("data-source-line=\"1\""), "got: {html}");
    assert!(html.contains("data-source-line=\"3\""), "got: {html}");
    assert!(html.contains("data-source-line=\"5\""), "got: {html}");
}

//! A colspan cell is always written as a plain cell (`| < |`), so a header row
//! whose spans form a TRAILING run keeps the native `|=` form - the span
//! absorbs into the header on its left and the `|=` markers still promote the
//! row. The writer used to fall back to a GFM delimiter row for ANY header
//! span, turning an ordinary colspan header into `| A | B | < |` +
//! `|---|---|---|` instead of the cleaner `|= A |= B | < |`.
//!
//! The delimiter row is still needed where the native form cannot promote the
//! row: a LEADING span (no `|=` anchor before it), a real cell AFTER a span
//! (which would read as `|=< K`, an aligned header), and a trailing ROWSPAN
//! (`^`, which does not absorb left). All three are pinned here.

use carve::{html_to_carve, parse, render_carve, to_html, HtmlImportMode, HtmlImportOptions};

fn imported(html: &str) -> String {
    html_to_carve(
        html,
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .expect("import")
    .value
}

fn fmt(source: &str) -> String {
    render_carve(&parse(source)).expect("write")
}

#[test]
fn imports_a_colspan_header_as_native_cells() {
    let html = "<table><caption>Results</caption>\
        <thead><tr><th>Engine</th><th colspan=\"2\">Timing</th></tr></thead>\
        <tbody><tr><td>carve-js</td><td>12ms</td><td>ok</td></tr></tbody></table>";
    assert_eq!(
        imported(html),
        "|= Engine |= Timing | < |\n| carve-js | 12ms | ok |\n^ Results\n"
    );
}

#[test]
fn the_native_form_is_fmt_idempotent_and_renders_the_colspan() {
    let src = "|= Engine |= Timing | < |\n| carve-js | 12ms | ok |\n";
    assert_eq!(fmt(src), src);
    assert!(to_html(src).contains("colspan=\"2\""));
}

#[test]
fn a_leading_span_keeps_the_delimiter_row() {
    let src = "| < | b |\n|---|---|\n| c | d |\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn a_real_cell_after_a_header_span_keeps_the_delimiter_row() {
    let src = "| H | < | < | K |\n|---|---|---|---|\n| p | q | s | t |\n";
    assert!(fmt(src).contains("|---|---|---|---|"));
}

#[test]
fn a_trailing_rowspan_header_keeps_the_delimiter_row() {
    let src = "| A | ^ |\n|---|---|\n| x | y |\n";
    assert!(fmt(src).contains("|---|---|"));
}

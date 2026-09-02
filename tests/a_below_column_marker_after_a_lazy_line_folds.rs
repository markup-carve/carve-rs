//! A LIST MARKER BELOW EVERY LIVE CONTENT COLUMN IS LAZY ITEM TEXT, and stays
//! that way when an ordinary lazy line is written above it
//! (markup-carve/carve-rs#1514).
//!
//! §10 is symmetric: no list marker interrupts a paragraph, and `parse_list`
//! already says so - a marker indented past the list's base but below the
//! content column folds. `collect_trailing_lazy_through` broke on EVERY marker,
//! so once an ordinary lazy line had handed the rest of the run to that
//! collector the marker escaped and opened a sub-list.
//!
//! WITHOUT THE LAZY LINE THE SAME DOCUMENT WAS ALREADY RIGHT, which is why
//! carve-rs#1509 measured the marker and ordered-marker kinds at 0 across all
//! 306 (prefix, column) pairs and called the folding marker a measured
//! survivor. The lazy line is what routes the marker through the other
//! collector.
//!
//! MEASURED, NOT ASSUMED. The carve-rs#1509 sweep re-run with one `b` line
//! inserted above each kind line: 306 prefix/column pairs, sixteen line kinds,
//! 4896 documents, `carve::to_html` against the executable spec at the pinned
//! corpus (`tests/spec` at carve `86569bd`). Before: the list-marker and
//! ordered-marker kinds disagreed on 4 pairs each and every other kind on 0.
//! After: 0 everywhere. The sweep WITHOUT the lazy line stays at 0 throughout.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

#[test]
fn the_reported_document_folds_the_marker() {
    assert_eq!(
        both_paths("- - - x\n b\n - y\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>\n",
            "        <ul>\n",
            "          <li>x\n",
            "b\n",
            "- y</li>\n",
            "        </ul>\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn every_pair_the_sweep_found_folds() {
    // The four (prefix, column) pairs, in the sweep's notation: `lll` col 1,
    // `llll` col 1, `llll` col 3, `qlll` col 3. An ordered marker diverged on
    // exactly the same four, so both spellings are asserted.
    for (marker, folded) in [("- y", "\nb\n- y</li>"), ("1. y", "\nb\n1. y</li>")] {
        for (prefix, lazy) in [
            ("- - - x", " b"),
            ("- - - - x", " b"),
            ("- - - - x", "   b"),
            ("> - - - x", ">  b"),
        ] {
            let indent = &lazy[..lazy.len() - 1];
            let src = format!("{prefix}\n{lazy}\n{indent}{marker}\n");
            let html = both_paths(&src);
            assert!(
                html.contains(folded),
                "the marker did not fold: {src:?}: {html}"
            );
            assert!(
                !html.contains("<li>y</li>"),
                "the marker opened an item: {src:?}: {html}"
            );
        }
    }
}

#[test]
fn a_sibling_marker_at_the_list_s_base_still_ends_it() {
    // THE CONTROL. A marker at the LIST's own base column is a sibling and must
    // still end the lazy run - a rule widened to every marker takes this with
    // it. The executable spec's own output for this document.
    assert_eq!(
        both_paths("- - - x\n b\n- y\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>\n",
            "        <ul>\n",
            "          <li>x\n",
            "b</li>\n",
            "        </ul>\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "  <li>y</li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_marker_at_a_live_content_column_still_opens() {
    // THE OTHER CONTROL. At column 2 the marker REACHES the outermost item and
    // opens a sibling inside it, exactly as carve-rs#1509 ruled for every other
    // opener in that band.
    let html = both_paths("- - - x\n b\n  - y\n");
    assert!(html.contains("<li>y</li>"), "{html}");
    assert!(!html.contains("b\n- y"), "{html}");
}

#[test]
fn the_marker_folds_with_no_lazy_line_too() {
    // Unchanged by this branch, and the reason the ticket needed the `b`: the
    // marker alone was always taken by the other collector, which folds it.
    let html = both_paths("- - - x\n - y\n");
    assert!(html.contains("x\n- y</li>"), "{html}");
    assert!(!html.contains("<li>y</li>"), "{html}");
}

#[test]
fn a_deeper_lazy_column_folds_the_same_way() {
    // The band is every column below the innermost content column, not column 1
    // alone. Column 3 under `- - - x` answers as column 1 does.
    assert_eq!(
        both_paths("- - - x\n   b\n   - y\n"),
        both_paths("- - - x\n b\n - y\n"),
    );
}

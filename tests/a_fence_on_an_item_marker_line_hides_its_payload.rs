//! A list marker that starts an item opens a body, so an unterminated fence on
//! its line interrupts no paragraph and a definition below it is payload for
//! both definition prepasses (PART 0 O1, carve-rs#3507).

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src).trim().to_string()
}

#[test]
fn a_nested_or_sibling_marker_line_fence_keeps_its_payload() {
    let rows = [
        "r[^d]\n\n- a\n  - ```x\n    [^d]: u\n",
        "r[^d]\n\n- a\n- ```x\n  [^d]: u\n",
        "r[^d]\n\n1. a\n   - ```x\n     [^d]: u\n",
        "[a][d]\n\n- a\n  1. ```x\n     [d]: u\n",
        "r[^d]\n\n> - a\n>   - ```x\n>     [^d]: u\n",
        "r[^d]\n\n> - ~~~\n>   [^d]: u\n",
    ];
    for src in rows {
        let out = html(src);
        assert!(
            !out.contains("fnref1") && !out.contains("href=\"u\""),
            "{src:?}: {out}"
        );
        assert!(out.contains("]: u"), "{src:?}: {out}");
    }
}

/// With no list open above, the marker folds into the paragraph (I2) and the
/// fence on its line opens nothing.
#[test]
fn a_folded_marker_opens_no_fence() {
    assert_eq!(
        html("[a][d]\n\np\n- ```x\n[d]: u\n"),
        "<p><a href=\"u\">a</a></p>\n<p>p\n- <code>x</code></p>"
    );
}

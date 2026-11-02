//! A line block's payload line opens nothing in a footnote body either, so a
//! fence- or definition-shaped payload keeps the stanza open for a lazy line
//! (carve#4748, PART 2 `line_block`).

use carve::to_html;

#[test]
fn a_block_shaped_payload_keeps_a_note_stanza_open() {
    for (opener, payload) in [
        ("::: |", "~~~"),
        (":::: |", "~~~"),
        ("::: |", "{.k}"),
        ("::: |", "[x]: /u"),
        ("::: |", "# h"),
    ] {
        let source = format!("r[^1]\n\n[^1]: {opener}\n  {payload}\nb\n");
        let html = to_html(&source);
        assert!(
            html.contains(&format!("<p>{payload}<br>\nb</p>")),
            "{source}\n{html}"
        );
        assert!(!html.contains("<p>b</p>"), "{source}\n{html}");
    }
}

#[test]
fn a_marker_line_note_keeps_a_fence_shaped_payload_in_its_stanza() {
    let html = to_html("r[^1]\n\n- [^1]: ::: |\n    ~~~\nb\n");
    assert!(html.contains("<p>~~~<br>\nb</p>"), "{html}");
}

#[test]
fn a_closed_line_block_lets_a_fence_open_after_it() {
    let html = to_html("r[^1]\n\n[^1]: ::: |\n  x\n  :::\n  ~~~\n  code\n  ~~~\n");
    assert!(html.contains("<pre><code>code\n</code></pre>"), "{html}");
}

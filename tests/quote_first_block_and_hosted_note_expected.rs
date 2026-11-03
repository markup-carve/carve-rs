//! Pins the Expected sections of carve-rs#3513 and carve-rs#3519.

use carve::to_html;

fn squeeze(html: &str) -> String {
    html.split('\n').map(str::trim).collect::<Vec<_>>().join("")
}

#[test]
fn a_stacked_quote_first_block_reads_its_follower_at_the_raw_column() {
    assert_eq!(
        to_html("> > +\n > q\n").trim(),
        "<blockquote>\n  <blockquote><p>+\nq</p></blockquote>\n</blockquote>"
    );
    assert_eq!(
        squeeze(&to_html("- > > +\n    > q\n")),
        "<ul><li><blockquote><blockquote><p>+q</p></blockquote></blockquote></li></ul>"
    );
}

#[test]
fn a_flush_marker_under_a_description_hosted_note_folds_into_it() {
    let html = to_html("R[^a].\n\n:: t\n: [^a]: first\n- i\n");
    assert!(html.contains("<p>first<a href=\"#fnref1\""), "{html}");
    assert!(html.contains("</dl>\n<ul>\n  <li>i</li>\n</ul>"), "{html}");
}

#[test]
fn a_fence_after_a_note_sub_list_stays_in_the_note() {
    let html = squeeze(&to_html(
        "R[^a].\n\n- [^a]: first\n   - i\n    ```\n    code\n    ```\n",
    ));
    assert!(
        html.contains("<li><ul><li>i</li></ul><pre><code>code</code></pre></li></ul>"),
        "{html}"
    );
}

#[test]
fn a_nested_note_definition_takes_the_following_lazy_line() {
    let html = to_html("Ref[^a].\n\n[^a]: first\n    lazy\n     [^b]: note b\n      more\n");
    assert!(html.contains("first\nlazy"), "{html}");
    assert!(!html.contains("<p>more"), "{html}");
}

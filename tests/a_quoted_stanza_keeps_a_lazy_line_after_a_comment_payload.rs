//! A comment payload line leaves a quoted line block's stanza open, so the
//! next lazy line joins it (carve#4748, PART 2 `line_block`).

use carve::to_html;

#[test]
fn a_comment_payload_keeps_the_quoted_stanza_open() {
    for (source, stanza) in [
        ("> ::: |\n> %% c\nb\n", "<p><br>\nb</p>"),
        ("> > ::: |\n> > x\n> > %%\nb\n", "<p>x<br>\n<br>\nb</p>"),
        ("> - ::: |\n>   %% c\nb\n", "<p><br>\nb</p>"),
        (":: t\n: > ::: |\n  > %% c\nb\n", "<p><br>\nb</p>"),
    ] {
        let html = to_html(source);
        assert!(html.contains(stanza), "{source}\n{html}");
        assert!(!html.contains("<p>b</p>"), "{source}\n{html}");
    }
}

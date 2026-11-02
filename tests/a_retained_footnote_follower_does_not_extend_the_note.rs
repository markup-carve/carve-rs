use carve::{to_html, to_html_with_options, Options};

// Rule A (markup-carve/carve#3885): the closerless `%%%` is a `%%` line below the
// note's column and retains nothing, so both lines after it are one paragraph.
#[test]
fn a_closerless_comment_fence_below_a_note_retains_no_follower() {
    for source in [
        "[^c]: para\n%%%\nc\ntail\n",
        "[^c]: ::: note\n      para\n%%%\nc\ntail\n",
        "[^c]: - para\n%%%\nc\ntail\n",
    ] {
        for html in [
            to_html(source),
            to_html_with_options(source, &Options::default().with_positions(true)),
        ] {
            assert_eq!(html.trim(), "<p>c\ntail</p>", "{source:?}");
        }
    }
}

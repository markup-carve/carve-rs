//! A `: ` line below a stanza's open `%%%` span in a nested item opens the next
//! description; each list frame slices its own columns off the span's payload
//! (markup-carve/carve-rs#4026).

use carve::to_html;

#[test]
fn the_description_after_the_span_survives_at_every_depth() {
    for src in [
        "- a\n  :: t\n  : ::: |\n    v\n    %%%\n  : z\n",
        "- x\n  - a\n    :: t\n    : ::: |\n      v\n      %%%\n    : z\n",
        "- x\n  - y\n    - a\n      :: t\n      : ::: |\n        v\n        %%%\n      : z\n",
    ] {
        let html = to_html(src);
        assert!(
            html.contains("</dd>\n") && html.contains("<dd>z</dd>"),
            "{src:?}: {html}"
        );
    }
}

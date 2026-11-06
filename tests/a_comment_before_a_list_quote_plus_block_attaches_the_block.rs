//! A comment is invisible, so a list quote's `+` attaches the block it leads
//! (CARVE-P0-004, markup-carve/carve-rs#4008).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn the_block_after_a_closed_span_joins_the_quote() {
    let attached = "<ul><li><blockquote><p>after</p></blockquote></li></ul>";
    for source in [
        "- > +\n  %%%\n  %%%\n  after\n",
        "- > +\n  %%%\n  k\n  %%%\n  after\n",
        "- > +\n  %% c\n  %%%\n  %%%\n  after\n",
    ] {
        assert_eq!(flat(source), attached, "{source:?}");
    }
    assert_eq!(
        flat("- - > +\n    %%%\n    %%%\n    after\n"),
        "<ul><li><ul><li><blockquote><p>after</p></blockquote></li></ul></li></ul>"
    );
}

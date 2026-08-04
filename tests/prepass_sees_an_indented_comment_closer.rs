//! The definition prepasses and the block parser must agree on where a comment
//! fence ends (PART 9 §24 C3, markup-carve/carve#629).
//!
//! #574 taught `comment_fence_close_index` and the block parser that a `%%%`
//! closer sits at any column. The two line-based definition prepasses kept the
//! strict test, so they disagreed with the pass that decides: the block parser
//! closed the comment, the prepasses did not, and every definition after the
//! closer went unregistered - then came back as VISIBLE text, which is the one
//! outcome a definition may never have.
//!
//! Nothing in the corpus pairs an indented closer with a definition after it,
//! which is why the suite stayed green through that.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

#[test]
fn a_link_definition_after_an_indented_closer_registers() {
    let out = html("%%%\nhidden\n  %%%\n[r]: /u\n[r][]\n");
    assert_eq!(out, "<p><a href=\"/u\">r</a></p>");
    assert!(
        !out.contains("[r]: /u"),
        "the definition line leaked as text"
    );
}

#[test]
fn a_footnote_definition_after_an_indented_closer_registers() {
    let out = html("%%%\nhidden\n  %%%\n[^f]: note\nsee[^f]\n");
    assert!(
        out.contains("doc-noteref"),
        "reference did not resolve: {out}"
    );
    assert!(
        !out.contains("[^f]: note"),
        "the definition line leaked as text"
    );
}

#[test]
fn the_comment_body_is_still_hidden() {
    // The half #574 fixed, kept pinned here so a repair of the prepasses cannot
    // quietly undo it.
    assert!(!html("%%%\nhidden\n  %%%\n[r]: /u\n[r][]\n").contains("hidden"));
}

#[test]
fn a_definition_inside_the_comment_still_registers_nothing() {
    // The control, and the direction that matters most: a comment's body is
    // OPAQUE (#504). Widening the closer test must not widen the body.
    assert_eq!(html("%%%\n[^a]: note\n%%%\nsee[^a]\n"), "<p>see[^a]</p>");
    assert_eq!(html("%%%\n[r]: /u\n%%%\n[r][]\n"), "<p>[r][]</p>");
}

#[test]
fn a_flush_left_closer_still_works() {
    // The shape that already worked, so the change is an extension rather than
    // a replacement.
    assert_eq!(
        html("%%%\nhidden\n%%%\n[r]: /u\n[r][]\n"),
        "<p><a href=\"/u\">r</a></p>"
    );
}

#[test]
fn a_list_marker_inside_a_comment_seeds_no_content_column() {
    // A comment body is opaque for COLUMN tracking too, which the footnote
    // prepass already had and the link prepass did not. A `- hidden` inside the
    // comment seeded a content column that outlived the fence, so the indented
    // line after it was stripped to that phantom column and registered - while
    // the block parser sees a top-level line two columns in and reads it as
    // text. Registered-and-visible is the worst of the two answers.
    //
    // This half was reachable on main through a FLUSH-LEFT closer; the indented
    // closer above only removed the accident that was hiding it.
    for src in [
        "%%%\n- hidden\n  %%%\n  [r]: /u\n\n[r][]\n",
        "%%%\n- hidden\n%%%\n  [r]: /u\n\n[r][]\n",
    ] {
        assert_eq!(
            html(src),
            "<p>[r]: /u</p>\n<p>[r][]</p>",
            "phantom content column for {src:?}"
        );
    }
}

//! A block that renders to nothing contributes no line to its parent's body.
//!
//! A comment, a comment block and a non-HTML raw block all render as the empty
//! string. Pushing the separating newline before knowing that left a blank line
//! where the block stood. carve-php is the oracle here; carve-js carried the
//! same divergence.

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn a_comment_block_inside_a_div() {
    assert_eq!(
        html(":::\n%%%\nx\n%%%\nbody\n:::\n"),
        "<div>\n  <p>body</p>\n</div>"
    );
}

#[test]
fn a_comment_block_inside_an_admonition() {
    assert_eq!(
        html("::: note\n%%%\nx\n%%%\nbody\n:::\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>body</p>\n</aside>"
    );
}

#[test]
fn a_comment_block_inside_a_block_quote() {
    assert_eq!(
        html("> q\n> %%%\n> x\n> %%%\n> body\n"),
        "<blockquote>\n  <p>q</p>\n  <p>body</p>\n</blockquote>"
    );
}

#[test]
fn a_definition_body_that_renders_to_nothing_closes_on_its_own_line() {
    assert_eq!(
        html(":: t\n:  %%%\n   x\n   %%%\n"),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>"
    );
}

#[test]
fn an_abbreviation_definition_inside_a_div() {
    assert_eq!(
        html(":::\n*[HTML]: HyperText Markup Language\n\nbody\n:::\n"),
        "<div>\n  <p>*[HTML]: HyperText Markup Language</p>\n  <p>body</p>\n</div>"
    );
}

#[test]
fn a_container_holding_only_an_empty_block_renders_like_an_empty_one() {
    // The empty line inside an aside and a block quote is what every engine
    // already emits for a container with no content. This change must not
    // move it - only the blank line a rendered-to-nothing block left behind.
    assert_eq!(html(":::\n%%%\nx\n%%%\n:::\n"), html(":::\n:::\n"));
    assert_eq!(
        html("::: note\n%%%\nx\n%%%\n:::\n"),
        html("::: note\n:::\n")
    );
    assert_eq!(html("> %%%\n> x\n> %%%\n"), html(">\n"));
}

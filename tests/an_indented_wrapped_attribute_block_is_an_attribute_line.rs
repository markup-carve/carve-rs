//! PART 9 §15 A5 (CARVE-P0-024): an indented block that wraps is an attribute
//! line like a flush-left one. Its continuation lines belong to it at any
//! column, it interrupts an open paragraph, and the line after its `}` is not
//! lazy. Below a list item's column it ends the item.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

#[test]
fn at_the_document_level() {
    assert_eq!(
        html("a\n {.c\n }\npara\n"),
        "<p>a</p>\n<p class=\"c\">para</p>"
    );
    assert_eq!(html(" {.c\n }\npara\n"), "<p class=\"c\">para</p>");
    assert_eq!(
        html("x\n  {.a\n  .b}\ntail\n"),
        "<p>x</p>\n<p class=\"a b\">tail</p>"
    );
}

#[test]
fn in_a_quote() {
    assert_eq!(
        html("> a\n>  {.c\n> }\n> para\n"),
        "<blockquote>\n  <p>a</p>\n  <p class=\"c\">para</p>\n</blockquote>"
    );
    assert_eq!(
        html("> - a\n>    {.c\n>   }\npara\n"),
        "<blockquote>\n  <ul>\n    <li>a</li>\n  </ul>\n</blockquote>\n<p>para</p>"
    );
    // The single-line spelling leaves no paragraph open either.
    assert_eq!(
        html("> q\n>  {.k}\ntail\n"),
        "<blockquote><p>q</p></blockquote>\n<p>tail</p>"
    );
    // The closer lacks the quote's marker, so nothing wraps.
    assert_eq!(
        html("> a\n>  {.c\n}\npara\n"),
        "<blockquote><p>a\n{.c\n}\npara</p></blockquote>"
    );
}

#[test]
fn in_a_description_body() {
    assert_eq!(
        html(":: t\n: a\n   {.c\n}\npara\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>a</dd>\n</dl>\n<p>para</p>"
    );
}

#[test]
fn in_and_below_a_list_item() {
    assert_eq!(
        html("- a\n   {c\n}\nx\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>x</p>"
    );
    assert_eq!(
        html("- a\n {c\n}\nx\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p c=\"\">x</p>"
    );
    assert_eq!(
        html("- a\n   {.c\n   }\n  para\n"),
        "<ul>\n  <li>a\n    <p class=\"c\">para</p>\n  </li>\n</ul>"
    );
}

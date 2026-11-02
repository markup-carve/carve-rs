//! An unresolved footnote reference keeps its attribute run as the text typed, like
//! an unresolved reference link (markup-carve/carve#4816).

use carve::{render_carve, to_html, to_plain_text};

#[test]
fn the_run_renders_as_typed() {
    for (source, expected) in [
        (
            "x[^n]{.c} and [t][m]{.c}\n",
            "<p>x[^n]{.c} and [t][m]{.c}</p>",
        ),
        (
            "x[^n]{a=\"<b>\"}{.c}\n",
            "<p>x[^n]{a=\"&lt;b&gt;\"}{.c}</p>",
        ),
        ("x[^n]{ .c }\n", "<p>x[^n]{ .c }</p>"),
        (
            "|a[^n]{.c}|\n",
            "<table>\n  <tbody>\n    <tr><td>a[^n]{.c}</td></tr>\n  </tbody>\n</table>",
        ),
    ] {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}

#[test]
fn a_resolved_reference_still_takes_the_run_as_attributes() {
    let html = to_html("x[^n]{.c}\n\n[^n]: z\n");
    assert!(html.contains("class=\"c\""), "{html}");
    assert!(!html.contains("{.c}"), "{html}");
}

#[test]
fn the_writer_and_plain_text_keep_the_run() {
    let doc = carve::parse("x[^n]{ .c }\n");
    assert_eq!(render_carve(&doc).unwrap().trim(), "x[^n]{ .c }");
    assert_eq!(to_plain_text("x[^n]{ .c }\n").trim(), "x[^n]{ .c }");
}

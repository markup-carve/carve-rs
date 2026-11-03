//! A link definition on a footnote's marker line is hoisted like one on a body
//! line (PART 9R R1a). Expected HTML: the spec model at carve 0aa7eb40.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

const BACKLINK: &str =
    "<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a>";

fn notes(body: &str) -> String {
    format!(
        "<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>{body}{BACKLINK}</p>\n    </li>\n  </ol>\n</section>"
    )
}

const REF: &str = "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>";

#[test]
fn the_definition_resolves_a_reference_in_the_document() {
    assert_eq!(
        html("x[^n]\n\n[^n]: [A]: b\n\n[y][A]\n"),
        format!("{REF}\n<p><a href=\"b\">y</a></p>\n{}", notes(""))
    );
}

#[test]
fn a_body_line_below_it_is_the_notes_paragraph() {
    assert_eq!(
        html("x[^n]\n\n[^n]: [A]: b\n  more\n"),
        format!("{REF}\n{}", notes("more"))
    );
}

#[test]
fn it_holds_in_a_list_item_and_a_quote() {
    for source in [
        "- x[^n]\n\n  [^n]: [A]: b\n\n[y][A]\n",
        "> x[^n]\n>\n> [^n]: [A]: b\n\n[y][A]\n",
    ] {
        let out = html(source);
        assert!(
            out.contains("<p><a href=\"b\">y</a></p>"),
            "{source}: {out}"
        );
        assert!(!out.contains("[A]: b"), "{source}: {out}");
    }
}

#[test]
fn a_nested_note_marker_is_not_a_link_definition() {
    assert_eq!(
        html("x[^n]\n\n[^n]: [^m]: b\n"),
        format!("{REF}\n{}", notes(""))
    );
}

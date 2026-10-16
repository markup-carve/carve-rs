fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

const NOTE: &str = "<section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\"><p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p></li></ol></section>";
const LEAD: &str = "<p><a href=\"/u\">r</a> [^p] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>";

#[test]
fn a_definition_after_an_ordered_marker_folded_into_a_document_paragraph_stays_text() {
    assert_eq!(
        html("[r][] [^p] [^c]\n1. [^c]: para\n      [r]: /u\n"),
        "<p>[r][] [^p] [^c] 1. [^c]: para [r]: /u</p>"
    );
}

#[test]
fn a_definition_past_a_note_term_column_stays_term_text() {
    assert_eq!(
        html("[r][] [^p] [^c]\n[^c]: :: t\n   [r]: /u\n"),
        "<p>[r][] [^p] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\"><dl><dt>t [r]: /u</dt></dl><p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p></li></ol></section>"
    );
}

#[test]
fn a_definition_past_a_quoted_note_column_registers_in_the_note() {
    assert_eq!(
        html("[r][] [^p] [^c]\n> [^c]: para\n>   [r]: /u\n"),
        format!("{LEAD}<blockquote></blockquote>{NOTE}")
    );
}

#[test]
fn a_definition_past_a_description_note_column_registers_in_the_note() {
    assert_eq!(
        html("[r][] [^p] [^c]\n:: t\n: [^c]: para\n    [r]: /u\n"),
        format!("{LEAD}<dl><dt>t</dt><dd></dd></dl>{NOTE}")
    );
}

#[test]
fn a_definition_in_a_description_note_ends_its_paragraph_before_tail() {
    assert_eq!(
        html("[r][] [^p] [^c]\n:: t\n: [^c]: para\n    [r]: /u\n    tail\n"),
        format!("{LEAD}<dl><dt>t</dt><dd></dd></dl><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\"><p>para</p><p>tail<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p></li></ol></section>")
    );
}

#[test]
fn a_definition_after_a_bullet_marker_folded_into_a_document_paragraph_stays_text() {
    assert_eq!(
        html("[r][] [^p] [^c]\n- [^c]: para\n     [r]: /u\n"),
        "<p>[r][] [^p] [^c] - [^c]: para [r]: /u</p>"
    );
}

#[test]
fn a_nested_note_past_a_quote_hosted_note_column_registers() {
    assert_eq!(
        html("> [^c]: para\n>   [^p]: n\n"),
        "<blockquote></blockquote>"
    );
}

#[test]
fn a_definition_after_a_folded_list_line_in_a_document_paragraph_stays_text() {
    assert_eq!(html("d\n- para\n  [r]: /u\n"), "<p>d - para [r]: /u</p>");
}

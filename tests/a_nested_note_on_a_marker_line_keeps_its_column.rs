fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

const NOTES: &str = "<p>[r][] [^p] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol>";
const BACK_C: &str =
    "<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a>";
const BACK_D: &str =
    "<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a>";

#[test]
fn a_rule_at_the_nested_notes_body_column_opens_in_that_note() {
    assert_eq!(
        html("[r][] [^p] [^c] [^d]\n[^c]: [^d]: para\n        ---\n"),
        format!("{NOTES}<li id=\"fn1\"><p>{BACK_C}</p></li><li id=\"fn2\"><p>para</p><hr><p>{BACK_D}</p></li></ol></section>")
    );
}

#[test]
fn an_attribute_at_the_nested_notes_body_column_attaches_to_its_next_block() {
    assert_eq!(
        html("[r][] [^p] [^c] [^d]\n[^c]: [^d]: para\n        {#x}\n        tail\n"),
        format!("{NOTES}<li id=\"fn1\"><p>{BACK_C}</p></li><li id=\"fn2\"><p>para</p><p id=\"x\">tail{BACK_D}</p></li></ol></section>")
    );
}

#[test]
fn a_definition_at_the_outer_notes_body_column_ends_the_nested_note() {
    let source = "[r][] [^p] [^c] [^d]\n[^c]: [^d]: para\n  [r]: /u\n        tail\n";
    let expected = format!("<p><a href=\"/u\">r</a> [^p] <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p><section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\"><p>tail{BACK_C}</p></li><li id=\"fn2\"><p>para{BACK_D}</p></li></ol></section>");
    assert_eq!(html(source), expected);
}

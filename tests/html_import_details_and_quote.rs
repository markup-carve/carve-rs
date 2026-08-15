//! Two recognition upgrades for the HTML importer (markup-carve/carve#1210 P9).
//!
//! `<details>` had no branch, so the element unwrapped: the summary and the
//! body were flushed into the same inline run and a disclosure widget imported
//! as one paragraph whose first words were its summary, with nothing between
//! them. Carve already has `::: details`, and the bundled extension renders it
//! straight back to `<details>/<summary>`.
//!
//! `<q>` also unwrapped, and reported `element-unwrapped` while doing it. The
//! marks are the element's entire rendered effect, so writing them out is a
//! mapping rather than a loss, and it now reports nothing.

use carve::{
    html_to_ast, html_to_carve, BlockNode, Details, HtmlImportDiagnosticCode, HtmlImportOptions,
    Options,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

/// Through the details extension, which is what makes this a round trip rather
/// than a shape that merely looks right in the source.
fn rendered(carve_src: &str) -> String {
    let ext = Details::new();
    let options = Options::new().with_extension(&ext);
    carve::to_html_with_options(carve_src, &options)
        .trim()
        .to_string()
}

#[test]
fn a_disclosure_widget_keeps_its_summary() {
    let html = "<details><summary>More</summary><p>Body text.</p></details>";
    assert_eq!(imported(html), "::: details \"More\"\nBody text.\n:::\n");
    assert_eq!(
        rendered(&imported(html)),
        "<details>\n  <summary>More</summary>\n  <p>Body text.</p>\n</details>"
    );
}

/// The element arrives as an admonition, not as a generic div: a div would
/// render the summary as ordinary body text, which is the same loss under a
/// tidier name.
#[test]
fn the_imported_node_is_a_details_admonition() {
    let doc = html_to_ast(
        "<details><summary>More</summary><p>Body.</p></details>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    let [BlockNode::Admonition(a)] = doc.children.as_slice() else {
        panic!("expected one admonition, got {:?}", doc.children);
    };
    assert_eq!(a.kind, "details");
    assert_eq!(a.title.as_ref().map(Vec::len), Some(1));
    assert_eq!(a.children.len(), 1);
}

/// `open` decides whether the widget starts open, and the extension puts it
/// back on the tag. Dropping it would import a disclosure that starts open as
/// one that starts closed.
#[test]
fn the_open_state_survives() {
    let html = "<details open id=\"faq\"><summary>M</summary><p>B</p></details>";
    assert_eq!(imported(html), "{#faq open}\n::: details \"M\"\nB\n:::\n");
    assert!(
        rendered(&imported(html)).starts_with("<details id=\"faq\" open=\"\">"),
        "{}",
        rendered(&imported(html))
    );
    assert!(html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .report
        .diagnostics
        .is_empty());
}

/// A `<details>` with no summary is still a disclosure and still imports as
/// one; `::: details` with no title is the shape for it.
#[test]
fn a_summary_is_not_required() {
    assert_eq!(
        imported("<details><p>No summary.</p></details>"),
        "::: details\nNo summary.\n:::\n"
    );
}

/// HTML5 allows one summary. A second is not one, so it falls through to the
/// block walk and is reported there rather than silently becoming a title.
#[test]
fn a_second_summary_is_body_content_and_is_reported() {
    let result = html_to_ast(
        "<details><summary>A</summary><p>B</p><summary>C</summary></details>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![HtmlImportDiagnosticCode::ElementUnwrapped]
    );
    assert!(
        imported("<details><summary>A</summary><p>B</p><summary>C</summary></details>")
            .contains("\nC\n"),
        "the second summary's text must survive as body"
    );
}

#[test]
fn a_quotation_becomes_the_marks_it_renders_as() {
    let html = "<p>He said <q>hello there</q> loudly.</p>";
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    assert_eq!(
        result.value,
        "He said \u{201c}hello there\u{201d} loudly.\n"
    );
    assert!(
        result.report.diagnostics.is_empty(),
        "a deliberate mapping is not an unwrap: {:?}",
        result.report.diagnostics
    );
}

/// HTML5 leaves the marks to the user agent and every one of them alternates,
/// so a nested quotation takes the single pair. Repeating the outer pair would
/// render as `""inner""`.
#[test]
fn a_nested_quotation_alternates_the_pair() {
    assert_eq!(
        imported("<p><q>outer <q>inner</q> tail</q></p>"),
        "\u{201c}outer \u{2018}inner\u{2019} tail\u{201d}\n"
    );
    // The counter unwinds: a second top-level quotation is back to the double
    // pair rather than stuck on the single one.
    assert_eq!(
        imported("<p><q><q>a</q></q> <q>b</q></p>"),
        "\u{201c}\u{2018}a\u{2019}\u{201d} \u{201c}b\u{201d}\n"
    );
}

/// An id or a class on the element still has a home. A span keeps it without
/// inventing a node for the quotation itself.
#[test]
fn a_quotations_own_attributes_survive_in_a_span() {
    assert_eq!(
        imported("<p><q class=\"x\">t</q></p>"),
        "[\u{201c}t\u{201d}]{.x}\n"
    );
}

/// CONTROL. `cite` is a URL with no slot on a span and is still reported. The
/// quotation stopping being an unwrap must not turn its attributes silent too.
#[test]
fn a_quotations_cite_url_is_still_reported() {
    let result = html_to_ast(
        "<p><q cite=\"https://x.example\">t</q></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![HtmlImportDiagnosticCode::AttributeDropped]
    );
}

/// CONTROL. An element Carve genuinely cannot express still unwraps and still
/// says so, so this PR is not read as "the importer stopped reporting things".
#[test]
fn an_element_carve_cannot_express_still_reports_its_unwrap() {
    let result = html_to_ast("<p><ruby>x</ruby></p>", &HtmlImportOptions::default()).unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![HtmlImportDiagnosticCode::ElementUnwrapped]
    );
}

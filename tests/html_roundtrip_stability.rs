use carve::{html_to_ast, html_to_carve, render_carve, to_html, HtmlImportMode, HtmlImportOptions};

fn stable(html: &str) -> String {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let first = html_to_carve(html, &options).unwrap();
    let rendered = to_html(&first.value);
    let second = html_to_carve(&rendered, &options).unwrap();
    assert_eq!(to_html(&second.value), rendered, "{html}");
    rendered
}

#[test]
fn paragraphs_exposed_by_an_unwrapped_div_loosen_the_list() {
    let rendered = stable("<ul><li>x<div><p>a</p><p>b</p></div></li></ul>");
    assert!(rendered.contains("<p>a</p>"));
    assert!(rendered.contains("<p>b</p>"));
    assert!(!stable("<ul><li>x<ul><li>y</li></ul></li></ul>").contains("<p>"));
    assert!(!stable("<ul><li>x<pre><code>c\n</code></pre>tail</li></ul>").contains("<p>"));
}

#[test]
fn empty_change_markers_do_not_delay_whitespace_collapse() {
    assert_eq!(stable("<p>a <del></del><ins></ins> b</p>"), "<p>a b</p>");
    assert_eq!(
        stable("<p>a\u{00a0}<del></del><ins></ins>\u{00a0}b</p>"),
        "<p>a&nbsp;&nbsp;b</p>"
    );
    let ast = html_to_ast("<p>a <del></del> b</p>", &HtmlImportOptions::default()).unwrap();
    assert!(!carve::to_json(&ast.value).contains("\"type\":\"delete\""));
}

#[test]
fn an_empty_definition_term_is_preserved_raw_in_roundtrip_mode() {
    assert!(stable("<dl><dt>\u{000c}</dt><dd>d</dd></dl>").contains("<dt>\u{000c}</dt>"));
    let safe = html_to_carve(
        "<dl><dt> </dt><dd>d</dd></dl>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(to_html(&safe.value), "<p>d</p>");
    assert!(!safe.report.diagnostics.is_empty());
    assert!(!safe
        .report
        .diagnostics
        .iter()
        .any(|row| row.message.contains("no <dt> before it")));
    let ast = html_to_ast(
        "<dl><dt></dt><dd>d</dd></dl>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(render_carve(&ast.value).is_err());
}

#[test]
fn dropping_an_empty_term_keeps_definition_order() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let imported = html_to_carve(
            "<dl><dt>a</dt><dd>first</dd><dt></dt><dd>second</dd><dt>b</dt><dd>third</dd></dl>",
            &HtmlImportOptions {
                mode,
                ..Default::default()
            },
        )
        .unwrap();
        let html = to_html(&imported.value);
        assert!(html.find("first").unwrap() < html.find("second").unwrap());
        assert!(html.find("second").unwrap() < html.find("third").unwrap());
        assert!(imported
            .report
            .diagnostics
            .iter()
            .any(|row| row.message.contains("keep document order")));
    }
}

#[test]
fn raw_empty_terms_retract_descendant_loss_reports() {
    let html =
        "<dl onclick=\"x\" style=\"color:red\"><div id=\"group\"><dt><b></b></dt><dd><a href=\"javascript:x\">d</a></dd></div></dl>";
    let imported = html_to_carve(
        html,
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(imported
        .report
        .diagnostics
        .iter()
        .any(|row| row.code == carve::HtmlImportDiagnosticCode::RawPreserved));
    assert!(!imported.report.diagnostics.iter().any(|row| matches!(
        row.code,
        carve::HtmlImportDiagnosticCode::AttributeDropped
            | carve::HtmlImportDiagnosticCode::ElementUnwrapped
    )));
    let rendered = stable(html);
    for (name, value) in [("onclick", "x"), ("style", "color:red")] {
        assert!(rendered.contains(&format!("{name}=\"{value}\"")));
        assert!(imported.report.diagnostics.iter().any(|row| {
            row.code == carve::HtmlImportDiagnosticCode::AttributePreserved
                && row.path.as_deref() == Some("/dl[1]")
                && row.message.contains(name)
        }));
    }
    assert!(rendered.contains("href=\"javascript:x\""));
}

#[test]
fn a_break_only_term_is_not_an_empty_term() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let options = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        let imported =
            html_to_carve("<dl><dt><br></dt><dd>definition body</dd></dl>", &options).unwrap();
        assert_eq!(
            to_html(&imported.value),
            "<dl>\n  <dt><br>\n</dt>\n  <dd>definition body</dd>\n</dl>"
        );
        assert!(imported.report.diagnostics.is_empty());
    }
    stable("<dl><dt><br></dt><dd>definition body</dd></dl>");
}

#[test]
fn preformatted_text_keeps_its_whitespace() {
    let html = "<pre>a  <b>b</b>\n  c</pre>";
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let options = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        let pre = html_to_carve(html, &options).unwrap();
        assert!(to_html(&pre.value).contains("a  b\n  c"));
    }
    assert!(stable(html).contains("a  b\n  c"));
}

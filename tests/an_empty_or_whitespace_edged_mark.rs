//! An empty mark has no Carve spelling, a mark whose content starts or ends in
//! Carve whitespace takes the braced form, and a flagged bold-italic whose
//! content cannot hug `/*` is nested. The HTML importer drops an empty mark
//! without a row (ruling markup-carve/carve-rs#1719), keeping its attributes on
//! an empty span.

use carve::{from_json, html_to_carve, render_carve, to_html, HtmlImportOptions, RenderCarveError};

fn doc(mark: &str) -> carve::Document {
    let json = format!(
        r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"paragraph","children":[{{"type":"text","value":"a "}},{mark},{{"type":"text","value":" b"}}]}}]}}"#
    );
    from_json(&json).expect("decode")
}

fn html_of(mark: &str) -> String {
    carve::render_html(&doc(mark)).expect("render")
}

const MARKS: [&str; 9] = [
    "emphasis",
    "strong",
    "underline",
    "strike",
    "highlight",
    "superscript",
    "subscript",
    "insert",
    "delete",
];

#[test]
fn an_empty_mark_is_refused() {
    for t in MARKS {
        let result = render_carve(&doc(&format!(r#"{{"type":"{t}","children":[]}}"#)));
        assert!(
            matches!(result, Err(RenderCarveError::SourceUnspellable(_))),
            "{t}: {result:?}"
        );
    }
}

#[test]
fn a_whitespace_edged_mark_keeps_its_meaning() {
    let edges = [
        r#"{"type":"text","value":"\tx"}"#,
        r#"{"type":"text","value":"x\t"}"#,
        r#"{"type":"soft_break"},{"type":"text","value":"x"}"#,
        r#"{"type":"text","value":" x"}"#,
    ];
    for t in MARKS {
        for kids in edges {
            let mark = format!(r#"{{"type":"{t}","children":[{kids}]}}"#);
            let source = render_carve(&doc(&mark)).expect("spellable");
            assert_eq!(to_html(&source), html_of(&mark), "{t} {kids}: {source:?}");
        }
    }
}

#[test]
fn a_flagged_bold_italic_that_cannot_hug_its_delimiters_is_nested() {
    for kids in [
        r#"{"type":"text","value":" x"}"#,
        r#"{"type":"text","value":"x "}"#,
        r#"{"type":"text","value":"\tx"}"#,
    ] {
        let mark = format!(
            r#"{{"type":"strong","boldItalic":true,"children":[{{"type":"emphasis","children":[{kids}]}}]}}"#
        );
        let source = render_carve(&doc(&mark)).expect("spellable");
        assert!(!source.contains("/*"), "{kids}: {source:?}");
        assert_eq!(to_html(&source), html_of(&mark), "{kids}: {source:?}");
    }
    let hugging = r#"{"type":"strong","boldItalic":true,"children":[{"type":"emphasis","children":[{"type":"text","value":" x"}]}]}"#;
    assert_eq!(render_carve(&doc(hugging)).unwrap(), "a /*\u{a0}x*/ b\n");
}

#[test]
fn the_html_importer_drops_an_empty_mark_without_a_row() {
    for tag in [
        "em", "i", "strong", "b", "s", "strike", "u", "mark", "sub", "sup", "ins", "del",
    ] {
        let result = html_to_carve(
            &format!("<p>a <{tag}></{tag}> b</p>"),
            &HtmlImportOptions::default(),
        )
        .unwrap();
        assert_eq!(result.value, "a  b\n", "{tag}");
        assert!(
            result.report.diagnostics.is_empty(),
            "{tag}: {:?}",
            result.report.diagnostics
        );
    }
}

#[test]
fn the_html_importer_keeps_an_empty_marks_attributes_on_a_span() {
    let result = html_to_carve(
        r#"<p>a <em id="t"></em> b</p>"#,
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "a []{#t} b\n");
    assert!(to_html(&result.value).contains(r#"id="t""#));
}

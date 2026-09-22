//! An inline element the HTML leaves empty is dropped with no report row: its
//! delimiter pair, such as `{**}`, reads back as text (carve-rs#1719).

use carve::{html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions};

fn imported(html: &str, mode: HtmlImportMode) -> (String, Vec<(HtmlImportDiagnosticCode, String)>) {
    let options = HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    };
    let result = html_to_carve(html, &options).unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.path.clone().unwrap_or_default()))
        .collect();
    (result.value, rows)
}

macro_rules! dropped {
    ($($name:ident: $tag:literal,)*) => {
        $(
            #[test]
            fn $name() {
                let html = format!("<p>a<{0}></{0}>b</p>", $tag);
                for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic, HtmlImportMode::Roundtrip] {
                    let (carve, rows) = imported(&html, mode);
                    assert_eq!(carve, "ab\n", "{html} {mode:?}");
                    assert_eq!(rows, Vec::new(), "{html} {mode:?}");
                }
            }
        )*
    };
}

dropped! {
    strong: "strong",
    b: "b",
    em: "em",
    i: "i",
    strike: "s",
    strike_element: "strike",
    insertion: "ins",
    deletion: "del",
    underline: "u",
    highlight: "mark",
    superscript: "sup",
    subscript: "sub",
}

#[test]
fn an_element_emptied_by_a_dropped_child_is_dropped_too() {
    let (carve, _) = imported("<p>a<strong><em></em></strong>b</p>", HtmlImportMode::Safe);
    assert_eq!(carve, "ab\n");
}

#[test]
fn a_paragraph_left_empty_writes_nothing() {
    let (carve, rows) = imported(
        "<blockquote><p><em></em></p></blockquote>",
        HtmlImportMode::Safe,
    );
    assert_eq!(carve, ">\n");
    assert_eq!(rows, Vec::new());
}

/// The element carried nothing, but its attribute can still matter (an `id` is
/// a link target), so it moves onto an empty span and nothing is lost.
#[test]
fn an_attribute_on_it_moves_onto_an_empty_span() {
    let (carve, rows) = imported("<p>a<sup id=\"x\"></sup>b</p>", HtmlImportMode::Safe);
    assert_eq!(carve, "a[]{#x}b\n");
    assert!(rows.is_empty(), "{rows:?}");
}

/// Control: a space is content a reader sees, and `{* *}` spells it.
#[test]
fn a_space_inside_is_kept() {
    let (carve, _) = imported("<p>a<strong> </strong>b</p>", HtmlImportMode::Safe);
    assert_eq!(carve, "a{* *}b\n");
    assert_eq!(to_html(&carve).trim_end(), "<p>a<strong> </strong>b</p>");
}

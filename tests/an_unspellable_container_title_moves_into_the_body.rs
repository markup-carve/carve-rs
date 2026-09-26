//! A container title the quoted slot cannot spell (a `"` from text, an
//! attribute value or a link title, or a line break) becomes the body's first
//! paragraph on the exit that writes source, with `structure-unspellable`.
//! Before, one such `<summary>` failed the whole `carve migrate --from html`.

use carve::{
    html_to_ast, html_to_carve, parse, to_carve, BlockNode, HtmlImportDiagnosticCode,
    HtmlImportOptions,
};

fn imported(html: &str) -> (String, Vec<(HtmlImportDiagnosticCode, String)>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).expect("imports");
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.path.clone().unwrap_or_default()))
        .collect();
    (result.value, rows)
}

macro_rules! moved {
    ($($name:ident: $html:expr => $carve:expr, $path:expr,)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn imports_as_the_expected_bytes() {
                    assert_eq!(imported($html).0, $carve);
                }

                #[test]
                fn the_import_is_a_fixed_point_of_fmt() {
                    let carve = imported($html).0;
                    assert_eq!(to_carve(&carve), carve);
                }

                #[test]
                fn the_container_survives_with_the_title_in_its_body() {
                    let doc = parse(&imported($html).0);
                    let [BlockNode::Admonition(admonition)] = doc.children.as_slice() else {
                        panic!("not one admonition: {:?}", doc.children);
                    };
                    assert!(admonition.title.is_none());
                    assert!(matches!(admonition.children.first(), Some(BlockNode::Paragraph(_))));
                }

                #[test]
                fn the_move_is_reported() {
                    assert_eq!(
                        imported($html).1,
                        vec![(HtmlImportDiagnosticCode::StructureUnspellable, $path.to_string())]
                    );
                }

                #[test]
                fn the_ast_exit_keeps_the_title_and_reports_nothing() {
                    let result = html_to_ast($html, &HtmlImportOptions::default()).unwrap();
                    assert!(result.report.diagnostics.is_empty());
                    let [BlockNode::Admonition(admonition)] = result.value.children.as_slice() else {
                        panic!("not one admonition");
                    };
                    assert!(admonition.title.is_some());
                }
            }
        )*
    };
}

moved! {
    // Minimized from MDN's Baseline widget, the page that failed.
    attribute_value_in_a_summary:
        "<details><summary><span title=\"Supported in Chrome\">x</span></summary><p>b</p></details>"
        => "::: details\n[x]{title=\"Supported in Chrome\"}\n\nb\n:::\n", "/details[1]",
    quote_in_a_summary:
        "<details><summary>say \"hi\"</summary><p>b</p></details>"
        => "::: details\nsay \\\"hi\\\"\n\nb\n:::\n", "/details[1]",
    line_break_in_a_summary:
        "<details><summary>a<br>b</summary><p>b</p></details>"
        => "::: details\na\\\nb\n\nb\n:::\n", "/details[1]",
    quote_in_a_callout_title:
        "<div class=\"note\"><p class=\"admonition-title\">say \"hi\"</p><p>b</p></div>"
        => "::: note\nsay \\\"hi\\\"\n\nb\n:::\n", "/div[1]",
    link_title_in_an_aside_title:
        "<aside class=\"admonition note\"><p class=\"admonition-title\"><a href=\"/x\" title=\"t\">l</a></p><p>b</p></aside>"
        => "::: note\n[l](/x \"t\")\n\nb\n:::\n", "/aside[1]",
}

#[test]
fn a_spellable_title_stays_a_title() {
    let (carve, rows) = imported("<details><summary>G</summary><p>b</p></details>");
    assert_eq!(carve, "::: details \"G\"\nb\n:::\n");
    assert!(rows.is_empty());
}

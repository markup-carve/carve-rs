//! A braced opener is text while a braced span of its kind is open, so a braced
//! span inside one of its own kind has no spelling: the writer refuses the tree
//! and the HTML importer unwraps the inner span (ruling markup-carve/carve#2066,
//! markup-carve/carve-rs#1725).

use carve::{
    html_to_ast, html_to_carve, parse, render_carve, to_carve, to_html, to_json,
    HtmlImportDiagnosticCode, HtmlImportOptions, RenderCarveError,
};

fn imported(html: &str) -> (String, Vec<(HtmlImportDiagnosticCode, String)>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.path.clone().unwrap_or_default()))
        .collect();
    (result.value, rows)
}

macro_rules! imports {
    ($($name:ident: $html:expr => $carve:expr, $rows:expr,)*) => {
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
                fn every_unwrapped_span_is_reported() {
                    let expected: Vec<(HtmlImportDiagnosticCode, String)> = $rows
                        .iter()
                        .map(|path: &&str| (HtmlImportDiagnosticCode::StructureUnspellable, path.to_string()))
                        .collect();
                    assert_eq!(imported($html).1, expected);
                }
            }
        )*
    };
}

imports! {
    strong_ending_in_a_break: "<p>a<strong><b>x<br></b></strong>b</p>" => "a{*x\\\n*}b\n",
        ["/p[1]/strong[2]/b[1]"],
    superscript: "<p><sup><sup>x</sup></sup></p>" => "{^x^}\n",
        ["/p[1]/sup[1]/sup[1]"],
    insertion: "<p><ins><ins>x</ins></ins></p>" => "{+x+}\n",
        ["/p[1]/ins[1]/ins[1]"],
    through_another_kind: "<p>a<strong>b<em>c<b>x</b></em></strong>d</p>" => "a{*b{/cx/}*}d\n",
        ["/p[1]/strong[2]/em[2]/b[2]"],
    three_levels: "<p><sup>a<sup>b<sup>c</sup></sup></sup></p>" => "{^abc^}\n",
        ["/p[1]/sup[1]/sup[2]", "/p[1]/sup[1]/sup[2]/sup[2]"],
}

/// Control: where one level can be bare, the nesting is written and kept.
#[test]
fn a_spellable_nesting_is_kept() {
    assert_eq!(
        imported("<p>a<strong><b>x</b></strong>b</p>"),
        ("a{**x**}b\n".to_string(), vec![])
    );
    assert_eq!(
        to_html("a{**x**}b\n"),
        "<p>a<strong><strong>x</strong></strong>b</p>"
    );
}

/// The AST exit keeps both levels.
#[test]
fn the_ast_import_keeps_the_nesting() {
    let doc = html_to_ast(
        "<p><sup><sup>x</sup></sup></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert!(to_json(&doc).contains(r#"{"type":"superscript","children":[{"type":"superscript""#));
}

/// The writer refuses the same tree.
#[test]
fn the_writer_refuses_the_tree() {
    let doc = html_to_ast(
        "<p><sup><sup>x</sup></sup></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert!(matches!(
        render_carve(&doc),
        Err(RenderCarveError::SourceUnspellable(_))
    ));
}

/// Control: the writer accepts a braced span of another kind inside one.
#[test]
fn the_writer_accepts_different_kinds() {
    assert!(render_carve(&parse("a{*b{/cx/}*}d\n")).is_ok());
}

/// The marks the importer puts on spans never reach a caller.
#[test]
fn neither_exit_carries_a_mark() {
    let html = "<p>a<strong>b<em>c<b>x</b></em></strong>d <sup><sup>y</sup></sup></p>";
    let doc = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(!to_json(&doc).contains("\"pos\""));
}

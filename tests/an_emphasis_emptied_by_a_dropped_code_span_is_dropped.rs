//! An emphasis that held only a dropped empty code span is dropped with it: an
//! empty pair such as `{**}` reads back as text the HTML never had.

use carve::{html_to_carve, to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportOptions};

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

macro_rules! cases {
    ($($name:ident: $html:expr => $carve:expr, $read_back:expr, $rows:expr,)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn imports_as_the_expected_bytes() {
                    assert_eq!(imported($html).0, $carve);
                }

                #[test]
                fn the_import_reads_back_without_delimiter_text() {
                    assert_eq!(to_html(&imported($html).0).trim_end(), $read_back);
                }

                #[test]
                fn the_import_is_a_fixed_point_of_fmt() {
                    let carve = imported($html).0;
                    assert_eq!(to_carve(&carve), carve);
                }

                #[test]
                fn every_dropped_element_is_reported() {
                    let rows = imported($html).1;
                    let expected: Vec<(HtmlImportDiagnosticCode, String)> = $rows
                        .iter()
                        .map(|path: &&str| (HtmlImportDiagnosticCode::StructureUnspellable, path.to_string()))
                        .collect();
                    assert_eq!(rows, expected);
                }
            }
        )*
    };
}

cases! {
    strong_in_a_link: "<p><a href=\"u\"><strong><code></code></strong></a></p>"
        => "[](u)\n", "<p><a href=\"u\"></a></p>",
        ["/p[1]/a[1]/strong[1]/code[1]", "/p[1]/a[1]/strong[1]"],
    text_before_it: "<p><a href=\"u\">y<strong><code></code></strong></a></p>"
        => "[y](u)\n", "<p><a href=\"u\">y</a></p>",
        ["/p[1]/a[1]/strong[2]/code[1]", "/p[1]/a[1]/strong[2]"],
    nested: "<p><a href=\"u\"><em><strong><code></code></strong></em></a></p>"
        => "[](u)\n", "<p><a href=\"u\"></a></p>",
        ["/p[1]/a[1]/em[1]/strong[1]/code[1]", "/p[1]/a[1]/em[1]/strong[1]", "/p[1]/a[1]/em[1]"],
    insertion_in_a_span: "<p><span class=\"k\"><ins><code></code></ins>y</span></p>"
        => "[y]{.k}\n", "<p><span class=\"k\">y</span></p>",
        ["/p[1]/span[1]/ins[1]/code[1]", "/p[1]/span[1]/ins[1]"],
}

/// Control: an emphasis that keeps other content is kept.
#[test]
fn an_emphasis_with_other_content_is_kept() {
    let (carve, rows) = imported("<p><a href=\"u\"><strong>z<code></code></strong></a></p>");
    assert_eq!(carve, "[*z*](u)\n");
    assert_eq!(rows.len(), 1);
}

/// Control: an emptied link label is still a link, so the link is kept.
#[test]
fn an_emptied_link_is_kept() {
    let (carve, rows) = imported("<p><a href=\"u\"><code></code></a></p>");
    assert_eq!(carve, "[](u)\n");
    assert_eq!(rows.len(), 1);
}

/// Control: an emphasis the HTML left empty is not reported as emptied by the
/// span dropped before it.
#[test]
fn an_emphasis_that_was_already_empty_gets_no_row() {
    let (_, rows) = imported("<p><a href=\"u\"><code></code>y<strong></strong></a></p>");
    assert_eq!(rows.len(), 1);
}

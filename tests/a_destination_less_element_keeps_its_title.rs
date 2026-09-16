//! An `<a>` or `<img>` that names no destination comes back as its content,
//! and its surviving attributes come with it. The title has no slot left, so
//! it joins them on the span (markup-carve/carve-rs#1738).

use carve::{html_to_carve, HtmlImportDiagnosticCode, HtmlImportOptions};

fn imported(html: &str) -> (String, Vec<HtmlImportDiagnosticCode>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let codes = result.report.diagnostics.iter().map(|d| d.code).collect();
    (result.value, codes)
}

macro_rules! cases {
    ($($name:ident: $html:expr => $carve:expr,)*) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(imported($html).0, $carve);
            }
        )*
    };
}

cases! {
    a_link: "<p><a href=\"\" title=\"t q\">q</a></p>" => "[q]{title=\"t q\"}\n",
    a_link_with_no_href: "<p><a title=\"t\">q</a></p>" => "[q]{title=t}\n",
    a_link_with_an_id: "<p><a href=\"\" title=\"t q\" id=\"k\">q</a></p>" => "[q]{#k title=\"t q\"}\n",
    an_image: "<p><img src=\"\" alt=\"a b\" title=\"paren\"></p>" => "[a b]{title=paren}\n",
    an_image_with_no_alt: "<p><img src=\"\" title=\"t\"></p>" => "[]{title=t}\n",
    an_empty_title: "<p><a href=\"\" title=\"\">q</a></p>" => "[q]{title}\n",
}

/// Control: a link that names a destination keeps its title in the slot.
#[test]
fn a_link_with_a_destination_keeps_its_title() {
    assert_eq!(
        imported("<p><a href=\"u\" title=\"t\">q</a></p>").0,
        "[q](u \"t\")\n"
    );
}

/// Control: with no title the element still comes back as its bare content.
#[test]
fn no_title_leaves_the_content_bare() {
    assert_eq!(imported("<p><a href=\"\">q</a></p>").0, "q\n");
}

/// The unwrap is still reported, and the title is no longer a silent loss.
#[test]
fn the_unwrap_is_the_only_row() {
    assert_eq!(
        imported("<p><a href=\"\" title=\"t\">q</a></p>").1,
        vec![HtmlImportDiagnosticCode::ElementUnwrapped]
    );
}

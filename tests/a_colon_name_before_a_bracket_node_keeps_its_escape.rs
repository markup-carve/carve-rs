//! Text ending in `:name` before a node that writes `[` first would open an
//! inline extension, so the colon is escaped (markup-carve/carve#2068,
//! markup-carve/carve-rs#1726).

use carve::{
    html_to_carve, render_carve, to_carve, to_html, BlockNode, HtmlImportOptions, InlineNode,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

macro_rules! cases {
    ($($name:ident: $html:expr => $carve:expr,)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn imports_as_the_expected_bytes() {
                    assert_eq!(imported($html), $carve);
                }

                #[test]
                fn the_import_reads_back_as_the_html() {
                    assert_eq!(to_html(&imported($html)).trim_end(), $html);
                }

                #[test]
                fn the_import_is_a_fixed_point_of_fmt() {
                    let carve = imported($html);
                    assert_eq!(to_carve(&carve), carve);
                }
            }
        )*
    };
}

cases! {
    span: "<p>x :name<span class=\"k\">n</span></p>" => "x \\:name[n]{.k}\n",
    link: "<p>x :name<a href=\"u\">n</a></p>" => "x \\:name[n](u)\n",
    intraword: "<p>x:name<span class=\"k\">n</span></p>" => "x\\:name[n]{.k}\n",
    underscore_name: "<p>x :_<a href=\"u\">n</a></p>" => "x \\:_[n](u)\n",
}

/// Control: a digit-first name is not an extension, so no escape.
#[test]
fn a_digit_first_name_takes_no_escape() {
    assert_eq!(imported("<p>x :1<a href=\"u\">n</a></p>"), "x :1[n](u)\n");
}

/// Control: a space between the name and the node ends the name.
#[test]
fn a_space_before_the_node_takes_no_escape() {
    assert_eq!(
        imported("<p>x :name <a href=\"u\">n</a></p>"),
        "x :name [n](u)\n"
    );
}

/// Control: a bare colon names nothing.
#[test]
fn a_bare_colon_takes_no_escape() {
    assert_eq!(imported("<p>x :<a href=\"u\">n</a></p>"), "x :[n](u)\n");
}

#[test]
fn a_note_reference_escapes_the_colon() {
    let mut doc = carve::parse("x [^1]\n\n[^1]: note\n");
    let BlockNode::Paragraph(paragraph) = &mut doc.children[0] else {
        panic!("expected a paragraph");
    };
    let InlineNode::Text(text) = &mut paragraph.children[0] else {
        panic!("expected leading text");
    };
    text.value = "x :name".to_string();
    assert!(render_carve(&doc).unwrap().starts_with("x \\:name[^1]\n"));
}

/// Control: a node that does not open with `[` takes no escape.
#[test]
fn a_node_without_a_bracket_takes_no_escape() {
    assert_eq!(
        imported("<p>x :name<strong>n</strong></p>"),
        "x :name{*n*}\n"
    );
}

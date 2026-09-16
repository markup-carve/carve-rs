//! A caret ending a text node is escaped when the next node writes a bracket run
//! that would read back as an inline note (markup-carve/carve-rs#1710).

use carve::{
    html_to_carve, render_carve, to_carve, to_html, BlockNode, HtmlImportOptions, InlineNode,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

macro_rules! importer_cases {
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

importer_cases! {
    link: "<p>x ^<a href=\"u\">n</a></p>" => "x \\^[n](u)\n",
    span: "<p>x^<span class=\"k\">n</span></p>" => "x\\^[n]{.k}\n",
    semantic_span: "<p>x ^<abbr title=\"t\">n</abbr></p>" => "x \\^[n]{abbr=t}\n",
    after_a_bracket: "<p>x [^<span class=\"k\">n</span></p>" => "x [\\^[n]{.k}\n",
}

/// Control: an empty or blank label opens no note, so the caret stays bare.
#[test]
fn a_blank_link_label_keeps_the_bare_caret() {
    assert_eq!(imported("<p>x ^<a href=\"u\"></a></p>"), "x ^[](u)\n");
    assert_eq!(imported("<p>x ^<a href=\"u\"> </a></p>"), "x ^[ ](u)\n");
}

/// Control: an image writes `!` first, so no bracket follows the caret.
#[test]
fn an_image_keeps_the_bare_caret() {
    assert_eq!(
        imported("<p>x ^<img src=\"i\" alt=\"a\"></p>"),
        "x ^![a](i)\n"
    );
}

/// The same caret written from a tree, before a node only a source can build.
fn caret_before(source: &str) -> String {
    let citations = carve::Citations::new();
    let mut doc =
        carve::parse_with_options(source, &carve::Options::new().with_extension(&citations));
    let BlockNode::Paragraph(paragraph) = &mut doc.children[0] else {
        panic!("expected a paragraph");
    };
    let InlineNode::Text(text) = &mut paragraph.children[0] else {
        panic!("expected leading text");
    };
    text.value.push('^');
    render_carve(&doc).unwrap()
}

#[test]
fn a_note_reference_escapes_the_caret() {
    let written = caret_before("x [^1]\n\n[^1]: note\n");
    assert!(written.starts_with("x \\^[^1]\n"), "{written}");
}

#[test]
fn a_citation_escapes_the_caret() {
    let written = caret_before("x [@k]\n");
    assert!(written.starts_with("x \\^[@k]\n"), "{written}");
}

#[test]
fn a_reference_link_escapes_the_caret() {
    let written = caret_before("x [n][r]\n\n[r]: u\n");
    assert!(written.starts_with("x \\^[n][r]\n"), "{written}");
}

/// Control: a resolved cross-reference writes `</#`, so the caret stays bare.
#[test]
fn a_cross_reference_keeps_the_bare_caret() {
    let mut doc = carve::parse("x [n](#h)\n");
    let BlockNode::Paragraph(paragraph) = &mut doc.children[0] else {
        panic!("expected a paragraph");
    };
    let [InlineNode::Text(text), InlineNode::Link(link)] = &mut paragraph.children[..] else {
        panic!("expected text and a link");
    };
    text.value.push('^');
    link.from_crossref = true;
    assert_eq!(render_carve(&doc).unwrap(), "x ^</#h>\n");
}

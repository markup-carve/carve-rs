//! Carve has no spelling for an empty destination, so the Markdown importer
//! writes a link with none as its content and an image as its alt text
//! (markup-carve/carve-rs#1717). A title has no slot left either, so it moves
//! onto a span (markup-carve/carve-rs#1738).

use carve::{markdown_to_ast, markdown_to_carve, render_html, to_carve, to_html};

macro_rules! cases {
    ($($name:ident: $markdown:expr => $carve:expr,)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn imports_as_the_expected_bytes() {
                    assert_eq!(markdown_to_carve($markdown), $carve);
                }

                #[test]
                fn both_exits_render_alike() {
                    assert_eq!(
                        render_html(&markdown_to_ast($markdown)).unwrap(),
                        to_html(&markdown_to_carve($markdown))
                    );
                }

                #[test]
                fn the_import_is_a_fixed_point_of_fmt() {
                    let carve = markdown_to_carve($markdown);
                    assert_eq!(to_carve(&carve), carve);
                }
            }
        )*
    };
}

cases! {
    link: "a [x]() b\n" => "a x b\n",
    angle_brackets: "[z](<>)\n" => "z\n",
    whitespace_only: "[*s*](< >)\n" => "/s/\n",
    image: "![y]()\n" => "y\n",
    link_with_a_title: "[q](<> \"t q\")\n" => "[q]{title=\"t q\"}\n",
    image_with_a_title: "![a *b* `c` [d](u)](<> (paren))\n" => "[a b c d]{title=paren}\n",
    an_empty_title: "[q](<> \"\")\n" => "q\n",
    a_reference_with_a_title: "[w][r]\n\n[r]: <> \"t\"\n" => "[w]{title=t}\n",
}

/// Control: a link with a destination stays a link.
#[test]
fn a_link_with_a_destination_is_kept() {
    assert_eq!(markdown_to_carve("[ok](u)\n"), "[ok](u)\n");
}

/// Control: an image with a source stays an image.
#[test]
fn an_image_with_a_source_is_kept() {
    assert_eq!(markdown_to_carve("![y](i.png)\n"), "![y](i.png)\n");
}

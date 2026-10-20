//! Empty Markdown destinations retain their link or image shape in Carve.

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
    link: "a [x]() b\n" => "a [x]() b\n",
    angle_brackets: "[z](<>)\n" => "[z]()\n",
    whitespace_only: "[*s*](< >)\n" => "[/s/](%20)\n",
    image: "![y]()\n" => "![y]()\n",
    link_with_a_title: "[q](<> \"t q\")\n" => "[q]( \"t q\")\n",
    image_with_a_title: "![a *b* `c` [d](u)](<> (paren))\n" => "![a b c d]( \"paren\")\n",
    quoted_empty_title: "> [a](/u\n>\"\")\n" => "> [a](/u \"\")\n",
    quoted_quote_destination: "> [a](\n> \"\")\n" => "> [a](\"\")\n",
    an_empty_title: "[q](<> \"\")\n" => "[q]( \"\")\n",
    a_reference_with_a_title: "[w][r]\n\n[r]: <> \"t\"\n" => "[w]( \"t\")\n",
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

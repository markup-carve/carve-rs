//! The layout whitespace after a `<br>` is dropped on import, as the parser
//! drops it from the line the break opens (markup-carve/carve-rs#1706).

use carve::{html_to_ast, html_to_carve, parse, to_carve, to_html, HtmlImportOptions};

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
                fn the_import_is_a_fixed_point_of_fmt() {
                    let carve = imported($html);
                    assert_eq!(to_carve(&carve), carve);
                }

                #[test]
                fn both_exits_build_the_same_tree() {
                    let ast = html_to_ast($html, &HtmlImportOptions::default()).unwrap().value;
                    assert_eq!(ast.children, parse(&imported($html)).children);
                }
            }
        )*
    };
}

cases! {
    text_after_the_break: "<p>x<br> y</p>" => "x\\\ny\n",
    newline_and_indent: "<p>x<br>\n  y</p>" => "x\\\ny\n",
    tab: "<p>x<br>\ty</p>" => "x\\\ny\n",
    space_before_a_strike_closer: "<p><s>x<br> </s></p>" => "{~x\\\n~}\n",
    inside_an_emphasis: "<p><em>a<br> b</em></p>" => "/a\\\nb/\n",
    space_then_a_strong: "<p>x<br> <strong>y</strong></p>" => "x\\\n*y*\n",
}

/// Control: a no-break space is content and stays.
#[test]
fn a_no_break_space_after_the_break_stays() {
    let carve = imported("<p>x<br>\u{a0}y</p>");
    assert_eq!(carve, "x\\\n\u{a0}y\n");
    assert_eq!(to_html(&carve), "<p>x<br>\n&nbsp;y</p>");
}

/// Control: a space that does not start the line stays.
#[test]
fn a_space_after_a_closer_on_the_new_line_stays() {
    assert_eq!(imported("<p><s>x<br></s> y</p>"), "{~x\\\n~} y\n");
}

//! PART 11 §5: a lone bracket inside bracketed content, and a `(` that would
//! open a link destination after a paired `]`, are escaped in the minimal form
//! (markup-carve/carve#2357).

use carve::{html_to_carve, parse, to_carve, HtmlImportOptions};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("imports")
        .value
}

macro_rules! writes {
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
                fn the_written_source_reads_back_as_the_imported_tree() {
                    let carve = imported($html);
                    assert_eq!(to_carve(&to_carve(&carve)), carve);
                    assert!(!parse(&carve).children.is_empty());
                }
            }
        )*
    };
}

writes! {
    lone_brackets_in_spans:
        "<p><span class=b>[</span><a href=/x>edit</a><span class=b>]</span></p>"
        => "[\\[]{.b}[edit](/x)[\\]]{.b}\n",
    lone_brackets_in_link_text:
        "<p><a href=/y>a ] b [ c</a></p>" => "[a \\] b \\[ c](/y)\n",
    a_balanced_pair_stays_bare:
        "<p><span class=c>[x]</span> and <a href=/y>see [1] here</a></p>"
        => "[[x]]{.c} and [see [1] here](/y)\n",
    a_bracket_across_emphasis_pairs:
        "<p><span class=b>[<em>x</em>]</span></p>" => "[[/x/]]{.b}\n",
    top_level_text_is_not_covered:
        "<p>top [ lone and ] lone</p>" => "top [ lone and ] lone\n",
    a_paren_that_opens_a_destination:
        "<p>[a](b) and <span class=c>[a](u)</span></p>" => "[a]\\(b) and [[a]\\(u)]{.c}\n",
    // Flattening the ruby clones the run's nodes; the rule still reaches them.
    a_paren_beside_a_ruby:
        "<p>[a](b) <ruby>x<rt>y</rt></ruby></p>" => "[a]\\(b) xy\n",
    a_paren_that_opens_nothing_stays_bare:
        "<p>f(x) and (see above) and [a] (b) and [a](b c) and ](x)</p>"
        => "f(x) and (see above) and [a] (b) and [a](b c) and ](x)\n",
}

fn ingested(inline_json: &str) -> String {
    let json = format!(
        r#"{{"type":"document","children":[{{"type":"paragraph","children":[{inline_json}]}}],"srcByteLength":0}}"#
    );
    carve::render_carve(&carve::from_json(&json).expect("decode AST")).expect("write")
}

#[test]
fn a_destination_paren_is_found_across_adjacent_text_nodes() {
    let split = ingested(r#"{"type":"text","value":"x [a]"},{"type":"text","value":"(b) y"}"#);
    let whole = ingested(r#"{"type":"text","value":"x [a](b) y"}"#);
    assert_eq!(split, "x [a]\\(b) y\n");
    assert_eq!(split, whole);
}

/// Corpus 45-inline-extensions-6: an extension's reader stops at the first
/// `]`, so a `[` in its content pairs with nothing and needs no escape.
#[test]
fn inline_extension_content_has_no_lone_bracket() {
    let written = ingested(
        r#"{"type":"inline_extension","name":"foo","content":[{"type":"text","value":"a [b"}]},{"type":"text","value":" c]"}"#,
    );
    assert_eq!(written, ":foo[a [b] c]\n");
    assert_eq!(to_carve(":foo[a [b] c]\n"), ":foo[a [b] c]\n");
}

/// A run holding an empty code span is left to the search, whose answer here
/// escapes the bracket rather than the paren.
#[test]
fn a_run_with_an_empty_code_span_is_left_to_the_search() {
    let written = ingested(r#"{"type":"text","value":"x [a](b) y "},{"type":"code","value":""}"#);
    assert_eq!(written, "x \\[a](b) y ``\n");
}

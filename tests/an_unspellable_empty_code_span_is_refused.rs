//! An empty code span is an open backtick run, which ends only at the end of a
//! block or at a braced closer. Anywhere else the writer's output reads back as
//! a different tree, so the writer refuses it and the HTML importer drops it
//! (markup-carve/carve-rs#1705).
//!
//! The trees are built by parsing a `Q` placeholder span and emptying it, since
//! no Carve source builds them.

use carve::{
    from_json, html_to_carve, parse, render_carve, render_html, to_carve, to_json, Document,
    HtmlImportDiagnosticCode, HtmlImportOptions, RenderCarveError,
};

fn emptied(template: &str) -> Document {
    let json = to_json(&parse(template));
    assert_eq!(
        json.matches(r#""value":"Q""#).count(),
        1,
        "one placeholder span"
    );
    from_json(&json.replace(r#""value":"Q""#, r#""value":"""#)).unwrap()
}

macro_rules! refused {
    ($($name:ident: $template:expr,)*) => {
        mod refused {
            use super::*;
            $(
                #[test]
                fn $name() {
                    assert!(matches!(
                        render_carve(&emptied($template)),
                        Err(RenderCarveError::SourceUnspellable(_))
                    ));
                }
            )*
        }
    };
}

refused! {
    text_after_it_in_a_strike: "~`Q` y~\n",
    text_after_it_in_a_paragraph: "x`Q`y\n",
    attributes_at_the_end_of_a_strike: "~x `Q`{.c}~\n",
    attributes_at_the_end_of_a_paragraph: "x `Q`{.c}\n",
    a_soft_break_after_it: "x `Q`\ny\n",
    a_comment_after_it: "x `Q` %% c\n",
    the_end_of_a_link_label: "[x `Q`](u)\n",
    a_braced_closer_inside_a_link_label: "[{~x `Q`~}](u)\n",
    the_end_of_a_span_label: "[x `Q`]{.c}\n",
    the_end_of_an_inline_note: "a ^[x `Q`]\n",
    the_end_of_a_middle_table_cell: "| a | x `Q` | c |\n",
    a_braced_closer_inside_a_middle_table_cell: "| a | {~x `Q`~} | c |\n",
}

macro_rules! written {
    ($($name:ident: $template:expr => $carve:expr,)*) => {
        mod written {
            use super::*;
            $(
                mod $name {
                    use super::*;

                    #[test]
                    fn writes_the_expected_bytes() {
                        assert_eq!(render_carve(&emptied($template)).unwrap(), $carve);
                    }

                    #[test]
                    fn reads_back_as_the_tree() {
                        let tree = emptied($template);
                        let written = render_carve(&tree).unwrap();
                        assert_eq!(render_html(&parse(&written)).unwrap(), render_html(&tree).unwrap());
                    }
                }
            )*
        }
    };
}

written! {
    the_end_of_a_paragraph: "x `Q`\n" => "x ``\n",
    the_end_of_a_strike: "~x `Q`~\n" => "{~x ``~}\n",
    a_strike_with_text_after_it: "{~x `Q`~}y\n" => "{~x ``~}y\n",
    the_end_of_a_superscript: "{^x `Q`^}\n" => "{^x ``^}\n",
    the_end_of_an_insertion: "{+x `Q`+}\n" => "{+x ``+}\n",
    a_braced_strike_inside_a_strong: "*~x `Q`~ y*\n" => "*{~x ``~} y*\n",
    the_end_of_the_last_table_cell: "| a | x `Q` |\n" => "| a | x `` |\n",
    a_braced_strike_with_text_after_it_in_the_last_cell: "| a | {~x `Q`~}y |\n" => "| a | {~x ``~}y |\n",
    the_end_of_a_heading: "{#h}\n# x `Q`\n" => "{#h}\n# x ``\n",
}

macro_rules! formatted {
    ($($name:ident: $source:expr,)*) => {
        mod formatted {
            use super::*;
            $(
                mod $name {
                    use super::*;

                    #[test]
                    fn the_writer_accepts_the_parsed_tree() {
                        assert!(render_carve(&parse($source)).is_ok());
                    }

                    #[test]
                    fn formatting_keeps_the_tree() {
                        assert_eq!(render_html(&parse(&to_carve($source))).unwrap(), render_html(&parse($source)).unwrap());
                    }

                    #[test]
                    fn formatting_is_idempotent() {
                        let formatted = to_carve($source);
                        assert_eq!(to_carve(&formatted), formatted);
                    }
                }
            )*
        }
    };
}

// No source builds a refused tree: each row puts an empty span where a tree
// above is refused, and the parser reads what follows into the span.
formatted! {
    text_after_it_in_a_strike: "~`` y~\n",
    text_after_it_in_a_paragraph: "x``y\n",
    attributes_after_it: "x ``{.c}\n",
    a_soft_break_after_it: "x ``\ny\n",
    a_link_label: "[x ``](u)\n",
    a_braced_closer_inside_a_link_label: "[{~x ``~}](u)\n",
    a_middle_table_cell: "| a | x `` | c |\n",
    a_braced_strike: "{~x ``~}y\n",
    the_end_of_a_paragraph: "x ``\n",
    the_end_of_the_last_table_cell: "| a | x `` |\n",
}

fn imported(html: &str) -> (String, Vec<HtmlImportDiagnosticCode>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let codes = result.report.diagnostics.iter().map(|d| d.code).collect();
    (result.value, codes)
}

macro_rules! imports {
    ($($name:ident: $html:expr => $carve:expr, [$($code:ident),*],)*) => {
        mod imports {
            use super::*;
            $(
                #[test]
                fn $name() {
                    assert_eq!(imported($html), ($carve.to_string(), vec![$(HtmlImportDiagnosticCode::$code),*]));
                }
            )*
        }
    };
}

imports! {
    an_empty_strike_content: "<p><s><code></code></s></p>" => "{~``~}\n", [],
    the_end_of_a_strike: "<p><s>x<code></code></s></p>" => "{~x``~}\n", [],
    text_after_it_in_a_strike: "<p><s><code></code>y</s></p>" => "~y~\n", [StructureUnspellable],
    text_after_it_in_a_paragraph: "<p>x<code></code>y</p>" => "xy\n", [StructureUnspellable],
    the_end_of_a_link_label: "<p><a href=\"u\">z<code></code></a></p>" => "[z](u)\n", [StructureUnspellable],
    attributes: "<p><s><code class=\"c\"></code></s></p>" => "{~``~}\n", [AttributeDropped],
    trailing_whitespace_after_it: "<p><s>x<code></code> </s></p>" => "{~x``~}\n", [],
    a_middle_table_cell: "<table><tr><td><code></code></td><td>b</td></tr></table>" => "| | b |\n", [StructureUnspellable],
}

/// The AST exit keeps the span: only a writer loses it.
#[test]
fn the_ast_import_keeps_the_empty_span() {
    let doc = carve::html_to_ast("<p>x<code></code>y</p>", &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(to_json(&doc).contains(r#"{"type":"code","value":""}"#));
}

/// The AST exit keeps the whitespace the writing exit trims after a span.
#[test]
fn the_ast_import_keeps_trailing_whitespace() {
    let doc = carve::html_to_ast(
        "<p><s>x<code></code> </s></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert!(to_json(&doc).contains(r#"{"type":"code","value":""},{"type":"text","value":" "}"#));
}

//! A hard break that ends an emphasis puts the closer at the start of the next
//! line, where a bare closer does not close (markup-carve/carve-rs#1702).

use carve::{
    from_prosemirror, html_to_carve, parse, render_carve, to_carve, to_html, to_prosemirror,
    HtmlImportOptions,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

fn one_line(html: &str) -> String {
    html.replace('\n', "")
}

macro_rules! writer_cases {
    ($($name:ident: $source:expr => $formatted:expr,)*) => {
        mod writer {
            use super::*;
            $(
                mod $name {
                    use super::*;

                    #[test]
                    fn writes_the_expected_bytes() {
                        assert_eq!(to_carve($source), $formatted);
                    }

                    #[test]
                    fn formatting_keeps_the_tree() {
                        assert_eq!(to_html(&to_carve($source)), to_html($source));
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

writer_cases! {
    strike: "{~x\\\n~}\n" => "{~x\\\n~}\n",
    emphasis: "{/x\\\n/}\n" => "{/x\\\n/}\n",
    strong: "{*x\\\n*}\n" => "{*x\\\n*}\n",
    underline: "{_x\\\n_}\n" => "{_x\\\n_}\n",
    highlight: "{=x\\\n=}\n" => "{=x\\\n=}\n",
    only_a_break: "{~\\\n~}\n" => "{~\\\n~}\n",
    two_breaks: "{~x\\\ny\\\n~}\n" => "{~x\\\ny\\\n~}\n",
    between_words: "a {~x\\\n~} b\n" => "a {~x\\\n~} b\n",
    in_a_list_item: "- {*x\\\n  *}\n" => "- {*x\\\n  *}\n",
    in_a_quote: "> {~x\\\n> ~}\n" => "> {~x\\\n> ~}\n",
    italic_around_a_strong: "{/{*x\\\n*}/}\n" => "/{*x\\\n*}/\n",
    strike_around_a_strong: "{~{*x\\\n*}~}\n" => "~{*x\\\n*}~\n",
    strong_around_an_italic: "{*{/x\\\n/}*}\n" => "*{/x\\\n/}*\n",
}

/// Control: text after the break leaves the closer on a word.
#[test]
fn a_break_before_text_keeps_the_bare_closer() {
    assert_eq!(to_carve("{~x\\\ny~}\n"), "~x\\\ny~\n");
}

macro_rules! importer_cases {
    ($($name:ident: $html:expr => $carve:expr,)*) => {
        mod importer {
            use super::*;
            $(
                mod $name {
                    use super::*;

                    #[test]
                    fn imports_as_the_expected_bytes() {
                        assert_eq!(imported($html), $carve);
                    }

                    #[test]
                    fn the_import_reads_back_as_the_html() {
                        assert_eq!(one_line(&to_html(&imported($html))), $html);
                    }

                    #[test]
                    fn the_import_is_a_fixed_point_of_fmt() {
                        let carve = imported($html);
                        assert_eq!(to_carve(&carve), carve);
                    }
                }
            )*
        }
    };
}

importer_cases! {
    strike_after_text: "<p><s>x<br></s></p>" => "{~x\\\n~}\n",
    strike_alone: "<p><s><br></s></p>" => "{~\\\n~}\n",
    strike_between_letters: "<p>a<s><br></s>b</p>" => "a{~\\\n~}b\n",
    emphasis: "<p><em>x<br></em></p>" => "{/x\\\n/}\n",
    strong_between_words: "<p>a <strong>x<br></strong> b</p>" => "a {*x\\\n*} b\n",
    highlight: "<p><mark>x<br></mark></p>" => "{=x\\\n=}\n",
    underline: "<p><u>x<br></u></p>" => "{_x\\\n_}\n",
    superscript: "<p><sup>x<br></sup></p>" => "{^x\\\n^}\n",
    subscript_between_letters: "<p>a<sub><br></sub>b</p>" => "a{,\\\n,}b\n",
    insertion: "<p><ins>x<br></ins></p>" => "{+x\\\n+}\n",
    deletion: "<p>(<del><br></del>)</p>" => "({-\\\n-})\n",
    two_breaks: "<p><s>x<br>y<br></s></p>" => "{~x\\\ny\\\n~}\n",
    link_label: "<p><a href=\"u\">x<br></a></p>" => "[x\\\n](u)\n",
    text_backslash_before_the_break: "<p><s>x\\<br></s></p>" => "{~x\\\\\\\n~}\n",
    italic_around_a_strong: "<p><em><strong>x<br></strong></em></p>" => "/{*x\\\n*}/\n",
    strike_around_a_strong: "<p><s><strong>x<br></strong></s></p>" => "~{*x\\\n*}~\n",
}

/// Control: an even backslash run is text and gains no newline.
#[test]
fn a_trailing_text_backslash_stays_text() {
    let html = "<p><s>x\\</s></p>";
    assert_eq!(one_line(&to_html(&imported(html))), html);
}

/// The ProseMirror bridge writes Carve through the same writer.
#[test]
fn a_prosemirror_strike_ending_in_a_break_writes_the_braced_closer() {
    let json = to_prosemirror(&parse("{~x\\\n~}\n")).json;
    let doc = from_prosemirror(&json).unwrap();
    assert_eq!(render_carve(&doc).unwrap(), "{~x\\\n~}\n");
}

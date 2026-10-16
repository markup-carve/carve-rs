//! A single `~` inside a GFM `~~` strike must not cost the strike
//! (markup-carve/carve-rs#2842). pulldown-cmark lets a tilde closer that finds
//! no opener of its own length block every later tilde closer, so the outer
//! `~~` pair fell back to literal text.

use carve::{markdown_to_carve, to_html, to_markdown};

fn imported_html(markdown: &str) -> String {
    to_html(&markdown_to_carve(markdown)).trim().to_string()
}

#[test]
fn a_strike_holding_a_lone_tilde_imports_as_a_strike() {
    for (markdown, html) in [
        ("~~a x~y b~~\n", "<p><s>a x~y b</s></p>"),
        ("~~a <x~y> b~~\n", "<p><s>a &lt;x~y&gt; b</s></p>"),
        ("~~a </x~y> b~~\n", "<p><s>a &lt;/x~y&gt; b</s></p>"),
        ("~~a </#x~>y> b~~\n", "<p><s>a &lt;/#x~&gt;y&gt; b</s></p>"),
    ] {
        assert_eq!(imported_html(markdown), html, "{markdown:?}");
    }
}

#[test]
fn nested_and_single_tilde_strikes_still_pair() {
    for (markdown, html) in [
        ("~~a ~~b~~ c~~\n", "<p><s>a <s>b</s> c</s></p>"),
        ("~a~\n", "<p><s>a</s></p>"),
        ("x~y~z\n", "<p>x<s>y</s>z</p>"),
        ("~a x~y b~\n", "<p><s>a x</s>y b~</p>"),
    ] {
        assert_eq!(imported_html(markdown), html, "{markdown:?}");
    }
}

#[test]
fn a_tilde_pair_inside_code_stays_code() {
    assert_eq!(
        imported_html("~~a `~~` b~~\n"),
        "<p><s>a <code>~~</code> b</s></p>"
    );
    assert_eq!(
        imported_html("~~a `x~y` b~~\n"),
        "<p><s>a <code>x~y</code> b</s></p>"
    );
}

#[test]
fn a_strike_holding_a_lone_tilde_survives_the_markdown_round_trip() {
    // Corpus 598-5.
    let carve = "{~a </#x~>y> b~}\n";
    let back = markdown_to_carve(&to_markdown(carve));
    assert_eq!(to_html(&back).trim(), "<p><s>a &lt;/#x~&gt;y&gt; b</s></p>");
}

#[test]
fn the_parser_alone_still_drops_the_strike() {
    // Control: without the importer's tilde pass, pulldown-cmark reads the
    // ticket input as text. If this starts failing, the pass may be removable.
    use pulldown_cmark::{html, Options, Parser};
    let mut out = String::new();
    html::push_html(
        &mut out,
        Parser::new_ext("~~a </#x~>y> b~~\n", Options::ENABLE_STRIKETHROUGH),
    );
    assert_eq!(out.trim(), "<p>~~a &lt;/#x~&gt;y&gt; b~~</p>");
}

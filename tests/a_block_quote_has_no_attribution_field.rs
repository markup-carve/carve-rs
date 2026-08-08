//! `BlockQuote::attribution` is gone (carve-rs#832), and these rows pin the
//! facts that made removing it safe, so re-adding it cannot happen quietly.
//!
//! The field was `Option<Vec<InlineNode>>` and was never set to `Some`
//! anywhere: the one parse-side assignment was `attribution: None`, and the
//! decoder line that could have set it sat BENEATH the unknown-field check,
//! which refuses `attribution` exactly as it refuses a name nobody has ever
//! used. So its writer, its decoder, its footnote collection, its id walk, its
//! profile filter, its three extension walks, its canonical-writer escape pass
//! and its two inline-coalescing sites were all unreachable, and a reader of
//! `ast_json.rs` saw a field written, read back and walked, and reasonably
//! concluded attribution was part of the wire format.
//!
//! Nothing an author writes moves. A quote's attribution line is an ordinary
//! construct - a caption, or a second paragraph - and always was.

use carve::{from_json, to_json, Options};

fn published(source: &str) -> String {
    to_json(&carve::parse_with_options(
        source,
        &Options::default().with_positions(true),
    ))
}

const QUOTE_WITH_ATTRIBUTION: &str = r#"{"type":"document","children":[{"type":"block_quote","attribution":[{"type":"text","value":"A"}],"children":[{"type":"paragraph","children":[{"type":"text","value":"q"}]}]}],"srcByteLength":4}"#;

const QUOTE_WITH_A_NAME_NOBODY_USES: &str = r#"{"type":"document","children":[{"type":"block_quote","zzzbogus":[{"type":"text","value":"A"}],"children":[{"type":"paragraph","children":[{"type":"text","value":"q"}]}]}],"srcByteLength":4}"#;

const QUOTE_ALONE: &str = r#"{"type":"document","children":[{"type":"block_quote","children":[{"type":"paragraph","children":[{"type":"text","value":"q"}]}]}],"srcByteLength":4}"#;

#[test]
fn ingest_refuses_attribution_exactly_as_it_refuses_a_bogus_name() {
    // The removal changed nothing here: the decode site was shadowed by this
    // check before the field went, so the two payloads answered alike then too.
    let named = from_json(QUOTE_WITH_ATTRIBUTION)
        .expect_err("a block_quote carrying `attribution` was accepted");
    let bogus = from_json(QUOTE_WITH_A_NAME_NOBODY_USES)
        .expect_err("a block_quote carrying `zzzbogus` was accepted");

    assert!(
        named.to_string().contains("attribution"),
        "wrong message: {named}"
    );
    assert_eq!(
        named.to_string().replace("attribution", "PROPERTY"),
        bogus.to_string().replace("zzzbogus", "PROPERTY"),
        "the two are refused for different reasons"
    );
}

#[test]
fn the_same_payload_without_the_property_decodes() {
    let doc = from_json(QUOTE_ALONE).expect("a plain block_quote was refused");
    assert_eq!(
        carve::render_html(&doc).expect("the quote exceeded the render ceiling"),
        "<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn no_published_block_quote_carries_an_attribution_property() {
    // Both spellings an author reaches for: a caption line under a quote, and a
    // second paragraph inside it. Neither ever produced the field.
    for source in [
        "> q\n^ Attr\n",
        "> q\n>\n> Attr\n",
        "> q\n\n^ Attr\n",
        "> outer\n> > inner\n> > ^ Attr\n",
    ] {
        let json = published(source);
        assert!(
            !json.contains("attribution"),
            "{source:?} published an attribution property: {json}"
        );
    }
}

#[test]
fn a_quote_with_an_attribution_line_still_renders_the_way_it_did() {
    // `> q` / `^ Attr` is a captioned quote (PART 9 §4), not a quote carrying an
    // attribution node, on every target.
    assert_eq!(
        carve::to_html("> q\n^ Attr"),
        "<figure>\n  <blockquote><p>q</p></blockquote>\n  <figcaption>Attr</figcaption>\n</figure>"
    );
    assert_eq!(carve::to_markdown("> q\n^ Attr"), "> q\n\nAttr\n");
    assert_eq!(carve::to_plain_text("> q\n^ Attr"), "\"q\"\n\nAttr\n");
}

//! `BlockQuote::attribution` is back, and this time it is load-bearing
//! (PART 9 §4a, markup-carve/carve#1159). These rows pin that.
//!
//! History matters here, because the field name has meant two opposite things.
//! carve-rs#832 REMOVED an `attribution` field that no code path could ever
//! populate: the one parse-side assignment was `attribution: None`, and the
//! decoder line that could have set it sat BENEATH the unknown-field check, so
//! ingest refused `attribution` exactly as it refused a name nobody had ever
//! used. Its writer, its walks and its escape pass were all unreachable, and a
//! reader of `ast_json.rs` reasonably concluded attribution was part of the
//! wire format when it was not.
//!
//! §4a makes it real: `^ Attr` under a quote is that quote's attribution, not a
//! caption on a figure wrapping it. So every assertion the removal left behind
//! is inverted below, and the file keeps its job - if the field ever goes back
//! to being written-but-never-produced, these rows fail.

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
fn ingest_accepts_attribution_and_still_refuses_a_bogus_name() {
    let doc = from_json(QUOTE_WITH_ATTRIBUTION)
        .expect("a block_quote carrying `attribution` was refused");
    assert_eq!(
        carve::render_html(&doc).expect("the quote exceeded the render ceiling"),
        "<blockquote>\n  <p>q</p>\n  <footer>A</footer>\n</blockquote>"
    );

    let bogus = from_json(QUOTE_WITH_A_NAME_NOBODY_USES)
        .expect_err("a block_quote carrying `zzzbogus` was accepted");
    assert!(
        bogus.to_string().contains("zzzbogus"),
        "wrong message: {bogus}"
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
fn only_the_caption_spelling_publishes_an_attribution_property() {
    // A caption line under a quote IS its attribution. A nested quote takes one
    // the same way, and the marker sits one level OUT - `> ^ Attr` - because
    // the caption line has to follow a CLOSED block, and the inner quote is
    // what closes when the `> >` prefix stops.
    for source in [
        "> q\n^ Attr\n",
        // §4's attachment allowance: adjacent, or across exactly one blank line.
        "> q\n\n^ Attr\n",
        "> outer\n> > inner\n> ^ Attr\n",
    ] {
        let json = published(source);
        assert!(
            json.contains("\"attribution\""),
            "{source:?} published no attribution property: {json}"
        );
    }

    // The two near-misses. A second paragraph inside the quote is ordinary
    // content, and `> > ^ Attr` is a LAZY CONTINUATION of the inner paragraph,
    // so the caret never reaches the caption slot and lands in the text
    // verbatim.
    for source in ["> q\n>\n> Attr\n", "> outer\n> > inner\n> > ^ Attr\n"] {
        let json = published(source);
        assert!(
            !json.contains("\"attribution\""),
            "{source:?} published an attribution property: {json}"
        );
    }
}

#[test]
fn a_quote_with_an_attribution_line_renders_as_an_attribution() {
    // `> q` / `^ Attr` is a quote carrying an attribution (PART 9 §4a), not a
    // figure wrapping a quote, on every target.
    assert_eq!(
        carve::to_html("> q\n^ Attr"),
        "<blockquote>\n  <p>q</p>\n  <footer>Attr</footer>\n</blockquote>"
    );
    assert_eq!(carve::to_markdown("> q\n^ Attr"), "> q\n\nAttr\n");
    assert_eq!(carve::to_plain_text("> q\n^ Attr"), "\"q\"\n\nAttr\n");
    assert_eq!(carve::to_carve("> q\n^ Attr"), "> q\n^ Attr\n");
}

#[test]
fn the_attribution_survives_a_round_trip_through_json() {
    let doc = carve::parse("> q\n^ Attr");
    let back = from_json(&to_json(&doc)).expect("the published quote was refused on ingest");
    assert_eq!(
        carve::render_html(&back).expect("the quote exceeded the render ceiling"),
        carve::render_html(&doc).expect("the quote exceeded the render ceiling")
    );
}

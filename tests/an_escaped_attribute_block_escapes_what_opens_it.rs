//! PART 11 §2 over a literal attribute block: the escape goes on the character
//! that OPENS something, and only on it.
//!
//! §2 is a minimality requirement in both directions - "a character is escaped
//! IF AND ONLY IF omitting the escape would change the re-parsed AST" - so the
//! question a literal `{#id}` asks is not how many escapes look safe but which
//! single one the re-parse actually needs.
//!
//! Measured for markup-carve/carve-rs#1298, which reported this engine writing
//! `{\#id}` where the other two were read as writing `\{\#id}`. Re-measured
//! first-hand at carve-js `287ba07` and carve-php `9ed0127`, both at their own
//! `main`:
//!
//!   | engine   | canonical writer | `markdown_to_carve` |
//!   | -------- | ---------------- | ------------------- |
//!   | carve-rs | `{\#id}`         | `{\#id}`            |
//!   | carve-js | `{\#id}`         | `\{\#id}`           |
//!   | carve-php| `{\#id}`         | `\{\#id}`           |
//!
//! So all three WRITERS already agree on the minimal spelling, and the seam the
//! ticket saw is the text-level Markdown escaper in the other two. This engine
//! has no such path - `markdown_to_carve` builds a `Document` and lets the
//! canonical writer emit source - which is why it comes out minimal there too
//! (markup-carve/carve-rs#1289).
//!
//! This file exists so "bring carve-rs in line with the majority" cannot land
//! quietly: the majority reading was of a stale build, and the shorter spelling
//! is the conforming one.

/// The document every spelling below has to describe: a paragraph holding the
/// literal text `{#id}`, with no attribute block attached to anything.
const LITERAL: &str = "<p>{#id}</p>";

#[test]
fn a_bare_attribute_block_is_not_literal_text_so_something_must_be_escaped() {
    // The premise. Without it the whole question is idle: if `{#id}` already
    // read as text, §2 would forbid every escape here.
    assert_eq!(carve::to_html("{#id}\n"), "");
    assert_eq!(carve::to_html("{.cls}\n"), "");
    assert_eq!(carve::to_html("{key=v}\n"), "");
}

#[test]
fn escaping_the_hash_alone_is_enough() {
    // The SHORTEST spelling that re-parses to the literal, which under §2's
    // "if and only if" is therefore the conforming one.
    assert_eq!(carve::to_html("{\\#id}\n"), LITERAL);
    assert_eq!(carve::to_html("a {\\#id} b\n"), "<p>a {#id} b</p>");
}

#[test]
fn escaping_the_brace_alone_is_not_enough() {
    // The control that says the `#` escape is the load-bearing one rather than
    // either escape doing the job. `#id` is a tag wherever it stands, so the
    // brace is not what has to be neutralized here.
    let html = carve::to_html("\\{#id}\n");

    assert_ne!(html, LITERAL);
    assert!(
        html.contains("class=\"tag\""),
        "expected the interior to re-read as a tag, got: {html}"
    );
}

#[test]
fn the_longer_spelling_describes_the_same_document_which_is_what_makes_it_idle() {
    // `\{\#id}` is not WRONG about the document - it renders the same - it is
    // over-escaped. That is exactly §2's "only if" half, and it is invisible to
    // a render comparison, an idempotency check or a tree comparison that
    // forgives escaping. `the_writer_invents_no_escape_the_re_parse_does_not_
    // need` is the sweep that can see it; this is the named case.
    assert_eq!(carve::to_html("\\{\\#id}\n"), LITERAL);
    assert_eq!(carve::to_html("{\\#id}\n"), LITERAL);
}

#[test]
fn the_writer_spends_one_escape_and_puts_it_on_the_opener() {
    // Entering through an ingest rather than a parse, because that is the shape
    // the seam appears in: an importer hands the writer a bare text node with
    // no escaping decided yet.
    for (value, want) in [
        ("{#id}", "{\\#id}\n"),
        ("{#id} tail", "{\\#id} tail\n"),
        ("a {#id} b", "a {\\#id} b\n"),
        // Where the interior opens nothing, the BRACE is what does, and it is
        // the one that gets the escape. `{\key=v}` would not even be an escape:
        // `k` is not escapable, so the backslash would publish itself.
        ("{key=v}", "\\{key=v}\n"),
        ("{.cls}", "\\{.cls}\n"),
    ] {
        let json = format!(
            r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"paragraph","children":[{{"type":"text","value":"{value}"}}]}}]}}"#
        );
        let doc = carve::from_json(&json).expect("ingest");
        let written = carve::render_carve(&doc).expect("write");

        assert_eq!(written, want, "writing the literal text {value:?}");
        assert_eq!(
            carve::to_html(&written),
            carve::render_html(&doc).expect("render"),
            "the written source re-reads as a different document for {value:?}"
        );
    }
}

#[test]
fn the_markdown_importer_lands_on_the_same_spelling() {
    // This engine's Markdown path is AST-first, so it inherits the writer's
    // answer instead of running a delimiter table over text. The other two
    // engines escape the brace here as well, which is the seam #1298 reported.
    assert_eq!(
        carve::markdown_to_carve("{#id}\n\nprose {#id} tail\n"),
        "{\\#id}\n\nprose {\\#id} tail\n"
    );
}

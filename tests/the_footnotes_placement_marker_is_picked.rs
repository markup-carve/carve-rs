//! The `::: footnotes` placement marker is PICKED per document, not fixed.
//!
//! The HTML renderer places the endnotes section by writing a marker where the
//! `::: footnotes` block stood and splicing the section in at it once the whole
//! body is rendered. That marker used to be the FIXED string NUL +
//! `carve:footnotes-placement` + NUL, on the claim that NUL "cannot appear in
//! rendered HTML output".
//!
//! The claim was about the two DOORS, not about the marker: `normalize_source`
//! replaces an authored NUL and PART 12 §21 replaces an ingested one, so no
//! source and no AST-JSON could spell it. A tree built through the node API
//! passes through neither door, and carve-php recorded that exact string in a
//! host-built text node rendering a footnotes `div` in the middle of an
//! author's paragraph (markup-carve/carve-php#1087).
//!
//! carve-rs now picks the marker against the document, as the parser's
//! definition placeholder (markup-carve/carve-rs#1218) and the Markdown
//! target's escape carriers (markup-carve/carve-rs#1216) already do. Nothing
//! else moves: the section still renders after the whole body and is spliced
//! in, it still places at ANY depth, and no id moves.

use carve::ast::{BlockExtension, BlockNode, Document, InlineNode};
use carve::{
    parse, render_html, to_html, to_html_with_options, BeforeRenderContext, CarveExtension,
    Details, Options, RenderContext,
};

/// The fixed marker this renderer placed by until markup-carve/carve-rs#1245.
///
/// Spelled out here rather than imported: it is retired, and a test that
/// imported it would go green by deleting the constant instead of by closing
/// the hole.
const RETIRED_FIXED_MARKER: &str = "\u{0}carve:footnotes-placement\u{0}";

/// The renderer's PREFERRED marker - what a document that writes no private-use
/// character of its own is rendered with. Picking is what makes an authored
/// occurrence of THIS harmless too.
const PREFERRED_MARKER: char = '\u{e008}';

/// A parsed document whose first paragraph's first text node the host has
/// rewritten - the node API, which neither normalization door stands in front
/// of.
fn with_authored_text(source: &str, value: &str) -> Document {
    let mut doc = parse(source);
    let BlockNode::Paragraph(paragraph) = &mut doc.children[0] else {
        panic!("first block is a paragraph");
    };
    match &mut paragraph.children[0] {
        InlineNode::Text(text) => text.value = value.to_string(),
        other => panic!("first inline is text, got {other:?}"),
    }
    doc
}

/// THE POINT OF THE CHANGE. A host-built text node carrying the retired fixed
/// marker is TEXT: it must not place the endnotes section, and it must not
/// degrade to an empty footnotes `div` either.
///
/// This is the shape markup-carve/carve-php#1087 recorded. Against the fixed
/// marker the section was spliced into the middle of the paragraph.
#[test]
fn a_host_authored_fixed_marker_is_text_and_not_a_placement() {
    let doc = with_authored_text("X[^a].\n\n[^a]: a\n", RETIRED_FIXED_MARKER);
    let html = render_html(&doc).expect("render");

    let paragraph = html.split("</p>").next().expect("a paragraph");
    assert!(
        !paragraph.contains("doc-endnotes"),
        "the endnotes section was spliced into the author's paragraph: {html:?}"
    );
    assert!(
        !html.contains("<div class=\"footnotes\"></div>"),
        "the author's text degraded to an empty footnotes div: {html:?}"
    );
    // The section still renders, and still at the document end.
    assert!(
        html.find("</p>").expect("a paragraph") < html.find("doc-endnotes").expect("endnotes"),
        "the endnotes section left the document end: {html:?}"
    );
}

/// The same for the renderer's OWN preferred marker: swapping one fixed string
/// for another fixed code point would leave the hole exactly where it was.
#[test]
fn a_host_authored_preferred_marker_is_text_and_not_a_placement() {
    let doc = with_authored_text("X[^a].\n\n[^a]: a\n", &PREFERRED_MARKER.to_string());
    let html = render_html(&doc).expect("render");

    assert!(
        html.starts_with(&format!("<p>{PREFERRED_MARKER}")),
        "the author's character did not survive the render: {html:?}"
    );
    assert!(
        !html.contains("<div class=\"footnotes\"></div>"),
        "the author's character degraded to an empty footnotes div: {html:?}"
    );
    assert!(
        html.find("</p>").expect("a paragraph") < html.find("doc-endnotes").expect("endnotes"),
        "the endnotes section left the document end: {html:?}"
    );
}

/// A document that writes the preferred marker AND asks for a placement gets
/// both: the block places, the author's character stays where it was written.
#[test]
fn an_authored_marker_and_a_real_placement_do_not_trade_places() {
    let doc = with_authored_text(
        "X[^a].\n\n::: footnotes\n:::\n\n## After\n\n[^a]: a\n",
        &format!("before{PREFERRED_MARKER}after"),
    );
    let html = render_html(&doc).expect("render");

    assert!(
        html.starts_with(&format!("<p>before{PREFERRED_MARKER}after")),
        "the author's character moved or was consumed: {html:?}"
    );
    assert_eq!(
        html.matches("doc-endnotes").count(),
        1,
        "the section is placed once: {html:?}"
    );
    assert!(
        html.find("doc-endnotes").expect("endnotes") < html.find("<h2").expect("h2"),
        "the section did not place at the `::: footnotes` block: {html:?}"
    );
}

/// A fragment HANDED TO AN EXTENSION can be embedded, embedded in part, or
/// dropped, so the markers it writes are left out of the count the collision
/// check compares against. Leaving them in let a dropped marker pay for an
/// authored one, and the two cancelled into a check that reported no collision.
///
/// The extension here embeds its fragment, which is the case that must keep
/// working: the placement still places inside the disclosure, and the authored
/// character next to it is still the author's.
#[test]
fn a_placement_an_extension_renders_still_places_beside_an_authored_marker() {
    let extension = Details::new();
    let options = Options::new().with_extension(&extension);
    let source = format!(
        "{PREFERRED_MARKER}X[^a].\n\n::: details \"D\"\n::: footnotes\n:::\n:::\n\n## After\n\n[^a]: a\n"
    );
    let html = to_html_with_options(&source, &options);

    assert!(
        html.starts_with(&format!("<p>{PREFERRED_MARKER}X")),
        "the author's character did not survive the render: {html:?}"
    );
    let disclosure = html
        .split_once("<details>")
        .expect("a disclosure")
        .1
        .split_once("</details>")
        .expect("a closed disclosure")
        .0;
    assert!(
        disclosure.contains("doc-endnotes"),
        "the section left the disclosure: {html:?}"
    );
    assert_eq!(html.matches("doc-endnotes").count(), 1);
}

/// An extension that renders its children and THROWS THE RESULT AWAY - the
/// shape a real extension takes when it decides, after rendering, to emit
/// something else (a budget fallback, a static-mode substitution).
#[derive(Default)]
struct DiscardsWhatItRenders;

impl CarveExtension for DiscardsWhatItRenders {
    fn name(&self) -> &'static str {
        "discards"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        for block in &mut doc.children {
            let BlockNode::Admonition(admonition) = block else {
                continue;
            };
            if admonition.kind != "discards" {
                continue;
            }
            *block = BlockNode::Extension(BlockExtension {
                attrs: None,
                name: "discards".to_string(),
                children: std::mem::take(&mut admonition.children),
                summary: None,
                label: None,
                pos: None,
            });
        }
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != "discards" {
            return None;
        }
        // Rendered, then dropped on the floor.
        let _discarded = ctx.render_blocks(&node.children);
        Some("<p>dropped</p>".to_string())
    }
}

/// THE MARKERS OF A DISCARDED FRAGMENT MUST NOT BE COUNTED. One marker written
/// into a fragment the extension throws away, and one marker the AUTHOR wrote,
/// make the naive count match: one written, one standing, no collision
/// reported - and the author's character is then consumed as the placement.
///
/// Counting only markers on paths that reach the document keeps the count a
/// lower bound, so the authored one still makes `seen` exceed it.
#[test]
fn a_marker_an_extension_discarded_does_not_pay_for_an_authored_one() {
    let extension = DiscardsWhatItRenders;
    let options = Options::new().with_extension(&extension);
    let source =
        format!("{PREFERRED_MARKER}X[^a].\n\n::: discards\n::: footnotes\n:::\n:::\n\n[^a]: a\n");
    let html = to_html_with_options(&source, &options);

    assert!(
        html.starts_with(&format!("<p>{PREFERRED_MARKER}X")),
        "the author's character was consumed as the placement marker: {html:?}"
    );
    assert!(
        !html.contains("<div class=\"footnotes\"></div>"),
        "the author's character degraded to an empty footnotes div: {html:?}"
    );
    assert!(
        html.contains("<p>dropped</p>"),
        "the extension did not run: {html:?}"
    );
    // Nothing placed, so the section stays at the document end.
    assert!(
        html.find("dropped").expect("the extension's output")
            < html.find("doc-endnotes").expect("endnotes"),
        "the endnotes section left the document end: {html:?}"
    );
}

/// ANY-DEPTH PLACEMENT IS UNCHANGED. carve-js recognizes the placement node in
/// `ast.children` only, so a nested block is an ordinary div there; carve-rs
/// writes its marker from `render_admonition`, which runs at every depth, so a
/// nested block places. Nothing about picking the marker touches that.
#[test]
fn a_placement_inside_a_blockquote_still_places_inside_the_blockquote() {
    let html = to_html("X[^a].\n\n> ::: footnotes\n> :::\n\n[^a]: a\n");
    let quote = html
        .split_once("<blockquote>")
        .expect("a blockquote")
        .1
        .split_once("</blockquote>")
        .expect("a closed blockquote")
        .0;
    assert!(
        quote.contains("doc-endnotes"),
        "the section left the blockquote: {html:?}"
    );
    assert_eq!(html.matches("doc-endnotes").count(), 1);
}

/// ZERO IDS MOVE. A footnote body can derive two ids of its own: a duplicate
/// heading slug's `-N` suffix, and a titled admonition's `adm-N` accessible
/// name. Both are counted while the endnotes section renders, and the section
/// renders AFTER the whole body whether it is placed or appended, so placing it
/// leaves every id exactly where an appended render put it.
///
/// This is what rules out carve-js's in-place recognition for this engine:
/// rendering the section where the node stands moves it relative to the body,
/// and carries those two counters with it.
#[test]
fn the_ids_a_footnote_body_derives_are_where_a_document_end_render_put_them() {
    const HEAD: &str = "X[^a] Y[^b].\n\n::: note \"Titled\"\nfirst\n:::\n\n## Dup\n\n";
    const TAIL: &str =
        "## After\n\n[^a]: ## Dup\n\n[^b]: ::: note \"Also titled\"\n    second\n    :::\n";

    let placed = to_html(&format!("{HEAD}::: footnotes\n:::\n\n{TAIL}"));
    let appended = to_html(&format!("{HEAD}{TAIL}"));

    // The placement took effect, so the two renders really do differ in ORDER.
    assert!(
        placed.find("doc-endnotes").expect("endnotes") < placed.find(">After<").expect("After"),
        "the section did not place at the `::: footnotes` block: {placed:?}"
    );
    assert!(
        appended.find(">After<").expect("After") < appended.find("doc-endnotes").expect("endnotes"),
        "the appended section did not land at the document end: {appended:?}"
    );

    // Each id, pinned to the ELEMENT that carries it, in BOTH renders. A set
    // comparison alone would not see two elements trade ids.
    for element in [
        "<p class=\"admonition-title\" id=\"adm-1\">Titled</p>",
        "<p class=\"admonition-title\" id=\"adm-2\">Also titled</p>",
        "<section id=\"Dup\">",
        "<h2 id=\"Dup-2\">Dup</h2>",
        "<li id=\"fn1\">",
        "<li id=\"fn2\">",
        "<section id=\"After\">",
    ] {
        assert!(
            placed.contains(element),
            "{element} missing from {placed:?}"
        );
        assert!(
            appended.contains(element),
            "{element} missing from {appended:?}"
        );
    }

    // And nothing ELSE carries an id in one render and not the other.
    let (mut placed_ids, mut appended_ids) = (ids(&placed), ids(&appended));
    placed_ids.sort();
    appended_ids.sort();
    assert_eq!(
        placed_ids, appended_ids,
        "an id appeared or vanished between a placed and an appended section"
    );
}

/// Every `id="..."` value in `html`, in the order they were written.
fn ids(html: &str) -> Vec<String> {
    html.match_indices("id=\"")
        .map(|(at, opener)| {
            let rest = &html[at + opener.len()..];
            rest[..rest.find('"').expect("a closed attribute")].to_string()
        })
        .collect()
}

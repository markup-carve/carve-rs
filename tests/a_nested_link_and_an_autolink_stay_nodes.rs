//! A NESTED LINK AND AN AUTOLINK STAY NODES (PART 12 section 3a,
//! markup-carve/carve#817).
//!
//! "Links never nest" is a RENDERING rule: an anchor may not contain another
//! anchor. It binds the renderer, not the encoder. A link or an autolink inside
//! a link's label is serialized as the node the author wrote, and every renderer
//! unwraps it at the render seam exactly as it did before.
//!
//! Flattening at the encoder is strictly lossier than the case the section opens
//! with. An unresolved reference at least keeps enough to be written back; a
//! nested link's destination did not survive at all - `[[x](y)](z)` published as
//! a link to `z` whose only child is `x` has lost `y` from the tree, so `fmt` on
//! the parsed document wrote `[[x](y)](z)` back while `fmt` on the same document
//! taken through the AST wrote `[x](z)`. That is the section 6 round trip
//! failing, and it is what the first group below pins.
//!
//! WHAT CAN AND CANNOT FAIL HERE. No corpus golden pins this: every target still
//! unwraps, so a corpus pair would pass before and after and prove nothing. The
//! pins that CAN fail are the round trip on the two shapes, the AST-shape
//! expectation that a `link` and an `autolink` are admissible inside a link's
//! children, and - in the other direction - the rendered output of every target,
//! which must not move.

use carve::ast::*;

fn json(src: &str) -> String {
    carve::to_json(&carve::parse(src))
}

fn label_of(doc: &carve::Document) -> &[InlineNode] {
    match &doc.children[0] {
        BlockNode::Paragraph(p) => match &p.children[0] {
            InlineNode::Link(l) => &l.children,
            other => panic!("expected a link, got {other:?}"),
        },
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

/// Parse, serialize, decode: what a consumer of the tree actually receives.
fn published(src: &str) -> carve::Document {
    carve::from_json(&json(src)).expect("the encoder writes decodable JSON")
}

// ---------------------------------------------------------------------------
// The node survives, with everything on it.
// ---------------------------------------------------------------------------

/// The inner link keeps its destination, which flattening deleted outright.
#[test]
fn a_nested_link_reaches_the_wire_with_its_destination() {
    match &published("[[x](y)](z)\n").children[0] {
        BlockNode::Paragraph(_) => {}
        other => panic!("expected a paragraph, got {other:?}"),
    }
    match &label_of(&published("[[x](y)](z)\n"))[0] {
        InlineNode::Link(inner) => {
            assert_eq!(inner.href, "y");
            match &inner.children[0] {
                InlineNode::Text(t) => assert_eq!(t.value, "x"),
                other => panic!("expected the label text, got {other:?}"),
            }
        }
        other => panic!("expected a nested link node, got {other:?}"),
    }
}

/// An autolink flattened the same way came back as a bare URL, and that is a
/// DIFFERENT document: a bare URL stays literal where an autolink is a link.
#[test]
fn a_nested_autolink_reaches_the_wire_as_an_autolink() {
    match &label_of(&published("[<https://e.com>](z)\n"))[0] {
        InlineNode::AutoLink(a) => assert_eq!(a.href, "https://e.com"),
        other => panic!("expected an autolink node, got {other:?}"),
    }
}

/// THE NODE CARRIES NO NON-ANCHOR FLAG. Nothing on the wire marks the inner link
/// as unclickable; a consumer infers it from context. Asserted by comparing the
/// serialized inner node with the SAME link written outside a label: the two are
/// byte-identical, so there is no property the nesting adds. A flag would show up
/// here as a difference, and section 11 has just made this surface strict.
#[test]
fn the_published_node_is_the_one_the_author_wrote_outside_a_label_too() {
    let inside = json("[[x](y)](z)\n");
    let outside = json("[x](y)\n");
    let node =
        "{\"type\":\"link\",\"href\":\"y\",\"children\":[{\"type\":\"text\",\"value\":\"x\"}]}";
    assert!(inside.contains(node), "{inside}");
    assert!(outside.contains(node), "{outside}");
}

// ---------------------------------------------------------------------------
// Section 6: the round trip, which is the pin that can fail.
// ---------------------------------------------------------------------------

/// `fmt` on the source and `fmt` on the same document taken through the AST are
/// two spellings of one document, and they must agree.
#[test]
fn the_round_trip_writes_back_what_the_author_wrote() {
    for src in [
        "[[x](y)](z)\n",
        "[<https://e.com>](z)\n",
        "[pre [in](/i) post](/o)\n",
    ] {
        let direct = carve::to_carve(src);
        let through_ast = carve::render_carve(&published(src)).expect("renderable");
        assert_eq!(through_ast, direct, "round trip diverged on {src:?}");
        assert_eq!(direct.trim(), src.trim(), "fmt itself moved on {src:?}");
    }
}

// ---------------------------------------------------------------------------
// RENDERED OUTPUT DOES NOT MOVE. Every target still unwraps at the render seam.
// ---------------------------------------------------------------------------

#[test]
fn every_target_still_unwraps_the_nested_link() {
    let src = "[[x](y)](z)\n";
    assert_eq!(carve::to_html(src).trim(), "<p><a href=\"z\">x</a></p>");
    assert_eq!(carve::to_markdown(src).trim(), "[x](z)");
    assert_eq!(carve::to_plain_text(src).trim(), "x");
    // ANSI byte for byte: a `contains` here was too weak to see the inner link
    // start a SECOND styling run, which is what the seam prevents.
    assert_eq!(
        carve::to_ansi(src).trim(),
        "\u{1b}[4m\u{1b}[34mx\u{1b}[0m\u{1b}[2m (z)\u{1b}[0m"
    );
}

#[test]
fn every_target_still_unwraps_the_nested_autolink() {
    let src = "[a <https://e.com> b](z)\n";
    assert_eq!(
        carve::to_html(src).trim(),
        "<p><a href=\"z\">a https://e.com b</a></p>"
    );
    assert_eq!(carve::to_markdown(src).trim(), "[a https://e.com b](z)");
    assert_eq!(carve::to_plain_text(src).trim(), "a https://e.com b");
    assert_eq!(
        carve::to_ansi(src).trim(),
        "\u{1b}[4m\u{1b}[34ma https://e.com b\u{1b}[0m\u{1b}[2m (z)\u{1b}[0m"
    );
}

/// The seam keeps the DISPLAY rewrite too: a `mailto:` scheme the author wrote
/// is dropped from the visible text. That transformation is the renderer's, and
/// moving the fold must not have left it behind in the parser.
#[test]
fn the_render_seam_still_strips_a_mailto_scheme() {
    let src = "[a <mailto:x@e.com> b](z)\n";
    assert_eq!(
        carve::to_html(src).trim(),
        "<p><a href=\"z\">a x@e.com b</a></p>"
    );
    assert_eq!(carve::to_markdown(src).trim(), "[a x@e.com b](z)");
    // The PLAIN renderer is the quiet one: with no unwrap it prints the href
    // the author wrote, scheme and all, and every other shape in this file
    // renders the same either way. Without this line its seam is untested.
    assert_eq!(carve::to_plain_text(src).trim(), "a x@e.com b");
    assert_eq!(
        carve::to_ansi(src).trim(),
        "\u{1b}[4m\u{1b}[34ma x@e.com b\u{1b}[0m\u{1b}[2m (z)\u{1b}[0m"
    );
}

/// A label rendered THROUGH the AST renders the same as one rendered from the
/// parse, which is the other half of "the fold is at the seam": a consumer that
/// decodes the wire and renders gets the unwrapped anchor too.
#[test]
fn a_decoded_document_renders_the_unwrapped_anchor() {
    assert_eq!(
        carve::render_html(&published("[[x](y)](z)\n"))
            .expect("renderable")
            .trim(),
        "<p><a href=\"z\">x</a></p>"
    );
}

// ---------------------------------------------------------------------------
// Controls.
// ---------------------------------------------------------------------------

/// THE PRECEDENT IS INSIDE THIS RULE ALREADY: a `heading_ref` in a link was
/// exempt from the flattening for exactly this reason - it reaches the tree and
/// the renderers suppress the nested anchor. Unchanged.
///
/// This is the SECOND seam every renderer has: a cross-reference's label is
/// CLONED from its heading, so a link in the heading arrives inside the anchor
/// the reference itself becomes, on a path the authored label never takes.
/// Every target is asserted, not just HTML - a renderer can pass the authored
/// seam and nest an anchor on this one with the whole suite green.
#[test]
fn control_a_heading_ref_in_a_heading_still_resolves_inside_the_label() {
    let src = "# H [l](/u)\n\nSee </#H-l>\n";
    assert!(
        carve::to_html(src).contains("<p>See <a href=\"#H-l\">H l</a></p>"),
        "{}",
        carve::to_html(src)
    );
    assert!(
        carve::to_markdown(src).contains("See [H l](#H-l)"),
        "{}",
        carve::to_markdown(src)
    );
    assert!(
        carve::to_plain_text(src).contains("See H l"),
        "{}",
        carve::to_plain_text(src)
    );
    assert!(
        carve::to_ansi(src).contains("H l"),
        "{}",
        carve::to_ansi(src)
    );
}

/// An image and a code span in a label were never flattened at all, which is
/// what makes this an extension of an existing exemption rather than a new rule
/// about what a label may contain.
#[test]
fn control_an_image_and_a_code_span_in_a_label_are_untouched() {
    assert!(json("[![a](/i.png)](z)\n")
        .contains("{\"type\":\"image\",\"src\":\"/i.png\",\"alt\":\"a\"}"));
    assert!(json("[`c`](z)\n").contains("{\"type\":\"code\",\"value\":\"c\"}"));
}

/// An UNRESOLVED reference inside a label was already exempt (it kept `ref` and
/// `rawRef` so it could be written back), and it still is. The exemption test in
/// the fold is `ref_label.is_some() && href.is_empty()`, so a fix that widened
/// the wrong side would show here.
#[test]
fn control_an_unresolved_reference_in_a_label_is_unchanged() {
    let out = json("[a [missing][nope] b](z)\n");
    assert!(out.contains("\"ref\":\"nope\""), "{out}");
    assert!(out.contains("\"rawRef\":\"[missing][nope]\""), "{out}");
    assert_eq!(
        carve::to_html("[a [missing][nope] b](z)\n").trim(),
        "<p><a href=\"z\">a [missing][nope] b</a></p>"
    );
}

/// A link inside a FOOTNOTE body written inside a label is not nested at all -
/// the body renders in the endnotes section, outside any anchor - so it was
/// never folded and must not start being.
#[test]
fn control_a_link_in_a_footnote_body_inside_a_label_is_not_nested() {
    let html = carve::to_html("[lab^[see [t](/u)]](z)\n");
    assert!(html.contains("href=\"/u\""), "{html}");
}

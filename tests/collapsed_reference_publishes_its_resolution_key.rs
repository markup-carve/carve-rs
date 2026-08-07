//! A collapsed reference publishes the label it RESOLVES BY (PART 12 §3a,
//! markup-carve/carve#962).
//!
//! `ref` is defined as the derived label - "the label the reference resolves
//! by" - and the authored spelling is already kept in `rawRef`. This engine
//! published the authored spelling in BOTH, so the one field defined as the
//! resolution key named a string the reference did not resolve by, and a
//! consumer reading it had to re-derive the key the engine had just computed.
//!
//! ## Which key answered is the whole rule
//!
//! PART 9R R1 offers the heading index two keys IN ORDER: the label AS WRITTEN,
//! then its rendered plain text. They are the same string for a label carrying
//! no markup, which is every example the clause gives, which is why the second
//! was never separated out. `ref` follows the one that answered, so the
//! derivation fires for exactly the references whose authored spelling is not
//! what reached the heading.
//!
//! ## Not a blanket strip
//!
//! The case a blanket strip inverts is the authored `[label]: url` definition:
//! `linkDefs` keys on the label AS WRITTEN, case-sensitively, and never reaches
//! the heading index at all. The arithmetic settles it - FIVE corpus documents
//! carry a collapsed reference with a markup-bearing label and the three-way
//! comparison names exactly TWO. A blanket strip would have moved all five.
//!
//! ## Derived, not normalized
//!
//! Trimming, whitespace collapse, NFC and case folding belong to MATCHING and
//! stay inside `normalize_heading_label`. `[Getting Started][]` under
//! `# getting started` has always published `Getting Started`, and publishing
//! the folded key would rewrite every plain label in every document to make one
//! markup-bearing one right.

/// `(ref, rawRef, destination)` for the first link or image in the document.
fn reference(src: &str) -> (Option<String>, Option<String>, String) {
    let doc = carve::parse(src);
    fn walk(blocks: &[carve::BlockNode]) -> Option<(Option<String>, Option<String>, String)> {
        for block in blocks {
            if let Some(found) = match block {
                carve::BlockNode::Paragraph(p) => inline(&p.children),
                carve::BlockNode::Heading(h) => inline(&h.children),
                _ => None,
            } {
                return Some(found);
            }
        }
        None
    }
    fn inline(nodes: &[carve::InlineNode]) -> Option<(Option<String>, Option<String>, String)> {
        for node in nodes {
            match node {
                carve::InlineNode::Link(l) => {
                    return Some((l.ref_label.clone(), l.raw_ref.clone(), l.href.clone()))
                }
                carve::InlineNode::Image(i) => {
                    return Some((i.ref_label.clone(), i.raw_ref.clone(), i.src.clone()))
                }
                carve::InlineNode::Emphasis(e) => {
                    if let Some(found) = inline(&e.children) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(&doc.children).expect("a link or image node")
}

#[test]
fn a_markup_bearing_label_publishes_the_rendered_text_it_reached_the_heading_by() {
    // The reported document (corpus 275-2). The committed golden already said
    // which string it is: the href is a slug of the heading's RENDERED text.
    let (r, raw, href) = reference("# `code()` heading\n\n[`code()` heading][]\n");
    assert_eq!(r.as_deref(), Some("code() heading"));
    assert_eq!(raw.as_deref(), Some("[`code()` heading][]"));
    assert_eq!(href, "#code-heading");

    // Its sibling (corpus 275).
    let (r, raw, href) = reference("# *bold* heading\n\n[*bold* heading][]\n");
    assert_eq!(r.as_deref(), Some("bold heading"));
    assert_eq!(raw.as_deref(), Some("[*bold* heading][]"));
    assert_eq!(href, "#bold-heading");
}

#[test]
fn a_label_the_heading_matches_as_written_is_not_derived() {
    // THE ROW A BLANKET STRIP GETS WRONG INSIDE THIS BRANCH, and the one no
    // corpus document holds. The heading's rendered text IS the markup
    // characters, because they sit in a code span - so the label AS WRITTEN is
    // the key that answered, and it is the key that must be published, even
    // though the label itself carries markup and derives to something else.
    //
    // Without this, "always derive when the label carries markup" passes every
    // other assertion in this file and every corpus document.
    let (r, raw, href) = reference("# `*bold*`\n\n[*bold*][]\n");
    assert_eq!(r.as_deref(), Some("*bold*"));
    assert_eq!(raw.as_deref(), Some("[*bold*][]"));
    assert_eq!(href, "#bold");
}

#[test]
fn the_two_keys_are_offered_in_order_and_the_first_one_wins() {
    // R1 says IN ORDER, and only a document where BOTH keys answer can show
    // that the order is load-bearing. Two headings: one whose rendered text is
    // the label AS WRITTEN (the markup sits in a code span), one whose rendered
    // text is what the label DERIVES to. The as-written key is offered first,
    // so it wins - and it decides the DESTINATION as well as `ref`, which is
    // what makes the order observable in the HTML too.
    //
    // "Always derive when the label carries markup" sends this reference to the
    // other heading. That mutation passes every corpus document and every other
    // assertion in this file.
    let src = "# `*x*` heading\n\n# x heading\n\n[*x* heading][]\n";
    let (r, _, href) = reference(src);
    assert_eq!(r.as_deref(), Some("*x* heading"));
    assert_eq!(href, "#x-heading");
    assert!(
        carve::to_html(src).contains("id=\"x-heading-2\""),
        "the second heading is the one NOT referenced, and it exists"
    );
}

#[test]
fn the_published_key_is_derived_and_not_folded() {
    // The case that separates DERIVED from NORMALIZED inside this branch: the
    // label carries markup AND differs from the heading in case, so the
    // derivation fires and the folded key is a DIFFERENT string from the
    // derived one. Publishing `normalize_heading_label`'s output here passes
    // the plain-label case above and fails this.
    let (r, raw, href) = reference("# `code()` Heading\n\n[`code()` Heading][]\n");
    assert_eq!(r.as_deref(), Some("code() Heading"));
    assert_eq!(raw.as_deref(), Some("[`code()` Heading][]"));
    assert_eq!(href, "#code-Heading");
}

#[test]
fn a_plain_label_derives_to_itself() {
    // CONTROL, and the reason the second key went unnoticed: with no markup the
    // two keys are the same string, so the label as written answers and there is
    // nothing to derive.
    let (r, _, href) = reference("# getting started\n\n[getting started][]\n");
    assert_eq!(r.as_deref(), Some("getting started"));
    assert_eq!(href, "#getting-started");
}

#[test]
fn a_label_that_differs_only_in_case_still_publishes_what_the_author_wrote() {
    // DERIVED, NOT NORMALIZED. Case folding is MATCHING's job. Publishing the
    // folded key here would rewrite this label - and every other plain one in
    // every document - to make a markup-bearing one right.
    let (r, _, href) = reference("# getting started\n\n[Getting Started][]\n");
    assert_eq!(r.as_deref(), Some("Getting Started"));
    assert_eq!(href, "#getting-started");
}

#[test]
fn a_collapsed_reference_to_an_authored_definition_keeps_the_label_as_written() {
    // THE ROW A BLANKET STRIP INVERTS. A definition keys on the label AS
    // WRITTEN, case-sensitively, and the definition branch never consults the
    // heading index - so there is no derived key here and none is published.
    // Corpus 193 and 275-3 both pin it.
    let (r, _, href) = reference("[*bold*]: /x\n\nsee [*bold*][]\n");
    assert_eq!(r.as_deref(), Some("*bold*"));
    assert_eq!(href, "/x");

    // Even where a heading with the same wording exists, the definition wins
    // and the label stays as written (corpus 275-3).
    let (r, _, href) =
        reference("[*bold* heading]: /x\n\n# *bold* heading\n\n[*bold* heading][]\n");
    assert_eq!(r.as_deref(), Some("*bold* heading"));
    assert_eq!(href, "/x");
}

#[test]
fn an_unresolved_collapsed_reference_derives_nothing() {
    // CONTROL. Nothing answered, so there is no key to publish and the authored
    // spelling stands - and the node stays a link (PART 12 §3a).
    let (r, raw, href) = reference("[*nope*][]\n");
    assert_eq!(r.as_deref(), Some("*nope*"));
    assert_eq!(raw.as_deref(), Some("[*nope*][]"));
    assert_eq!(href, "");
}

#[test]
fn a_full_reference_is_never_offered_the_heading_index() {
    // CONTROL. `[text][label]` resolves through definitions only
    // (markup-carve/carve#742), so the index is not consulted and the label is
    // published as written even where a heading would have matched it.
    let (r, _, href) = reference("# *bold* heading\n\n[text][*bold* heading]\n");
    assert_eq!(r.as_deref(), Some("*bold* heading"));
    assert_eq!(href, "");
}

#[test]
fn the_rendered_html_and_the_canonical_source_do_not_move() {
    // `ref` is a WIRE field: every renderer tests it for PRESENCE and writes
    // `rawRef`. So the two documents that move publish byte-identical HTML and
    // byte-identical canonical source, which is why the HTML corpus could not
    // see this and the value ledger had to.
    for src in [
        "# `code()` heading\n\n[`code()` heading][]\n",
        "# *bold* heading\n\n[*bold* heading][]\n",
        "[*bold* heading]: /x\n\n# *bold* heading\n\n[*bold* heading][]\n",
    ] {
        let formatted = carve::to_carve(src);
        assert_eq!(carve::to_html(&formatted), carve::to_html(src));
        assert_eq!(carve::to_carve(&formatted), formatted);
        assert!(
            formatted.contains("[*bold* heading][]") || formatted.contains("[`code()` heading][]"),
            "the writer stopped reproducing the authored reference: {formatted}"
        );
    }
}

#[test]
fn the_derived_key_is_offered_to_the_index_as_well_as_published() {
    // R1 offers the index BOTH keys, so a heading carrying an id of its own is
    // reachable by its rendered text. This engine previously answered such a
    // document only through its slug fallback, which cannot fire once the id is
    // not the slug of the text - so the reference did not resolve at all.
    let (r, _, href) = reference("{#custom}\n# *bold* heading\n\n[*bold* heading][]\n");
    assert_eq!(href, "#custom");
    assert_eq!(r.as_deref(), Some("bold heading"));
}

#[test]
fn a_collapsed_image_reference_is_unchanged() {
    // CONTROL, and a scope boundary. The image branch resolves through
    // definitions only - it is never offered the heading index - so no derived
    // key exists on that path and the label stays as written.
    let (r, _, src_attr) = reference("# *bold* heading\n\n![*bold* heading][]\n");
    assert_eq!(r.as_deref(), Some("*bold* heading"));
    assert_eq!(src_attr, "");
}

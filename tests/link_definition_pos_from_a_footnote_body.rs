//! A link reference definition hoisted out of a FOOTNOTE BODY carries a `pos`.
//!
//! carve-rs#633 gave the definition a node and placed a `pos` on every path but
//! this one: a definition on a note body's continuation line hoisted with no
//! position at all. I wrote that deliberately, reasoning that the index the
//! extractor had was body-local and §4 prefers no position to a wrong one - but
//! the document line WAS available two lines further down, where `def_line_map`
//! already computes `first_source_line + i`. carve-js and carve-php both place it
//! (carve-rs#636).
//!
//! §4 requires a `pos` on every node but the root, and §10 is specific about this
//! case: a definition authored inside a container is a child of the DOCUMENT and
//! "its `pos` still says where it was written".
//!
//! OFF-BY-ONE IS THE TRAP. `first_source_line` is 1-based - a newline count plus
//! one - while `LinkDef.line` is the 0-based index `extract_link_defs` records.
//! Mixing the two silently points one line off, which still slices back to
//! SOMETHING, so the assertions below check the sliced text rather than the
//! numbers alone.

use carve::ast::BlockNode;

const NOTE_BODY_DEF: &str = "[^a]: note\n  [r]: /u\n\nsee[^a] [t][r]\n";

fn definitions(src: &str) -> Vec<(String, Option<(usize, usize)>)> {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(src, &options);
    doc.children
        .iter()
        .filter_map(|b| match b {
            BlockNode::LinkReferenceDefinition(d) => Some((
                d.label.clone(),
                d.pos.as_ref().map(|p| (p.start_offset, p.end_offset)),
            )),
            _ => None,
        })
        .collect()
}

/// The source the node's span claims, sliced by CODEPOINT (PART 12 offsets).
fn sliced(src: &str, span: (usize, usize)) -> String {
    src.chars().take(span.1).skip(span.0).collect()
}

#[test]
fn a_definition_from_a_footnote_body_has_a_position() {
    let defs = definitions(NOTE_BODY_DEF);
    assert_eq!(defs.len(), 1, "expected one definition, got {defs:?}");
    let span = defs[0].1.expect("the definition carries a pos");
    // The whole line INCLUDING its indentation, the same span carve-js and
    // carve-php produce for this input.
    assert_eq!(sliced(NOTE_BODY_DEF, span), "  [r]: /u");
}

#[test]
fn the_other_container_paths_still_have_one() {
    // These already worked; pinned so a fix for the note body cannot trade them.
    for src in [
        "[r]: /u\n\nsee [t][r]\n",
        "> [r]: /u\n\nsee [t][r]\n",
        "- [r]: /u\n\nsee [t][r]\n",
    ] {
        let defs = definitions(src);
        assert_eq!(defs.len(), 1, "{src:?} -> {defs:?}");
        let span = defs[0].1.unwrap_or_else(|| panic!("no pos for {src:?}"));
        assert!(
            sliced(src, span).contains("[r]: /u"),
            "{src:?} span sliced to {:?}",
            sliced(src, span)
        );
    }
}

#[test]
fn the_span_is_the_definition_line_not_the_line_above_it() {
    // The off-by-one guard. `first_source_line` is 1-based and `LinkDef.line` is
    // 0-based; using the former directly points at `[^a]: note`, which still
    // slices back to a plausible-looking string.
    let defs = definitions(NOTE_BODY_DEF);
    let span = defs[0].1.expect("pos");
    let text = sliced(NOTE_BODY_DEF, span);
    assert!(
        !text.contains("[^a]"),
        "span landed on the note line: {text:?}"
    );
    assert!(
        text.contains("[r]:"),
        "span missed the definition: {text:?}"
    );
}

#[test]
fn positions_off_still_yields_no_pos() {
    // The flag is still honored: nothing invents a position when the caller did
    // not ask for one.
    let doc = carve::parse(NOTE_BODY_DEF);
    let has_pos = doc.children.iter().any(|b| match b {
        BlockNode::LinkReferenceDefinition(d) => d.pos.is_some(),
        _ => false,
    });
    assert!(!has_pos, "a pos appeared with positions disabled");
}

#[test]
fn the_definition_still_resolves_and_still_round_trips() {
    // The behavior around the position is unchanged: the reference resolves, and
    // the definition line is written back where the author had it.
    let html = carve::to_html(NOTE_BODY_DEF);
    assert!(html.contains("href=\"/u\""), "{html}");
    assert_eq!(
        carve::to_html(&carve::to_carve(NOTE_BODY_DEF)),
        html,
        "the round trip changed the document"
    );
}

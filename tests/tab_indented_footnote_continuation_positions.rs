//! A tab-indented footnote continuation carries the same five positions the
//! space spelling does.
//!
//! `strip_leading_columns` is residual-aware: when a tab STRADDLES the column a
//! container strips to, it re-inserts the overshoot as spaces, so `<TAB>more`
//! dedented by two columns becomes `"  more"` - two characters written where
//! one was consumed. The column map then has to hold the constant that maps a
//! column in that result back to a column in the document, and that constant is
//! `-1`. `Vec<Option<usize>>` could not hold it, so `stripped_col` answered
//! `None` and the footnote, its paragraph, one soft break and one text node
//! published no `pos` at all (markup-carve/carve-rs#736).
//!
//! The map is signed now. The residual itself is UNTOUCHED, which is the point:
//! a whole-character dedent would also restore the positions, and it would move
//! a tab-indented fence and a tab-indented quote away from the oracle - so
//! those two shapes are pinned below as controls.

use carve::{BlockNode, InlineNode, Options, Pos};

/// `[^a]: note` / `<TAB>more` / blank / `see[^a]`.
const TAB: &str = "[^a]: note\n\tmore\n\nsee[^a]\n";
/// The same document with two spaces where the tab is. It always worked.
const SPACES: &str = "[^a]: note\n  more\n\nsee[^a]\n";

fn note_body(source: &str) -> Vec<BlockNode> {
    let doc = carve::parse_with_options(source, &Options::default().with_positions(true));
    doc.footnote_defs
        .get("a")
        .unwrap_or_else(|| panic!("no footnote `a` parsed out of {source:?}"))
        .clone()
}

type Span = Option<(usize, usize)>;
/// One typed node's span: `("text", Some((6, 10)))`.
type Placed = (&'static str, Span);

fn span(pos: &Option<Pos>) -> Span {
    pos.as_ref().map(|p| (p.start_offset, p.end_offset))
}

/// The note body's paragraph span, then one entry per inline in it. Together
/// these are four of the five nodes; the fifth is the `footnote` node the JSON
/// writer derives from the paragraph's own span, asserted separately.
fn body_spans(source: &str) -> (Span, Vec<Placed>) {
    let body = note_body(source);
    let [BlockNode::Paragraph(p)] = &body[..] else {
        panic!("the note body is not one paragraph: {body:?}");
    };
    let inlines = p
        .children
        .iter()
        .map(|n| match n {
            InlineNode::Text(t) => ("text", span(&t.pos)),
            InlineNode::SoftBreak(b) => ("soft_break", span(&b.pos)),
            other => panic!("unexpected inline in the note body: {other:?}"),
        })
        .collect();
    (span(&p.pos), inlines)
}

#[test]
fn the_tab_spelling_places_all_five_nodes() {
    // The offsets carve-js and carve-php both publish for this document.
    let (paragraph, inlines) = body_spans(TAB);
    assert_eq!(paragraph, Some((6, 16)), "the note body's paragraph");
    assert_eq!(
        inlines,
        vec![
            ("text", Some((6, 10))),
            ("soft_break", Some((10, 12))),
            ("text", Some((12, 16))),
        ]
    );
    // The fifth: the serialized `footnote` node, whose span the writer derives
    // from the first and last placed block of the body.
    let json = carve::to_json_with_options(TAB, &Options::default().with_positions(true));
    assert!(
        json.contains("\"type\":\"footnote\""),
        "no footnote node was serialized:\n{json}"
    );
    assert!(
        !json.contains("\"pos\":null"),
        "a node published a null position:\n{json}"
    );
}

#[test]
fn the_space_spelling_stays_placed() {
    // THE CONTROL THE RULING NAMES. A regression here would be worse than the
    // bug: the space spelling published all five positions before the map was
    // widened, and it has to keep publishing them after.
    let (paragraph, inlines) = body_spans(SPACES);
    assert_eq!(paragraph, Some((6, 17)), "the note body's paragraph");
    assert_eq!(
        inlines,
        vec![
            ("text", Some((6, 10))),
            ("soft_break", Some((10, 13))),
            ("text", Some((13, 17))),
        ]
    );
}

#[test]
fn every_text_span_slices_back_to_its_own_value() {
    // Stronger than "present": a span can be published and still name the wrong
    // characters, which is the failure a missing-position count cannot see.
    for source in [TAB, SPACES] {
        let chars: Vec<char> = source.chars().collect();
        let body = note_body(source);
        let [BlockNode::Paragraph(p)] = &body[..] else {
            panic!("the note body is not one paragraph");
        };
        for node in &p.children {
            let InlineNode::Text(t) = node else { continue };
            let (start, end) = span(&t.pos).expect("a placed text node");
            let slice: String = chars[start..end].iter().collect();
            assert_eq!(
                slice, t.value,
                "a text span names {slice:?} and the node says {:?} in {source:?}",
                t.value
            );
        }
    }
}

#[test]
fn a_tab_indented_fence_in_a_note_body_stays_literal() {
    // THE FIRST SHAPE THE WHOLE-CHARACTER DEDENT BROKE. The tab's overshoot is
    // relative indentation, so the fence is INDENTED, the strict column-0 rule
    // makes it literal, and it renders as an inline code span. Treating the tab
    // as atomic lands the fence flush left and opens a real code block.
    let html = carve::to_html("[^a]: note\n\n\t```\n\t  x\n\t```\n\nsee[^a]\n");
    assert!(
        html.contains("<code>") && !html.contains("<pre>"),
        "a tab-indented fence opened a real block:\n{html}"
    );
}

#[test]
fn a_tab_indented_quote_in_a_note_body_stays_literal() {
    // The second one, for the same reason.
    let html = carve::to_html("[^a]: note\n\n\t> q\n\nsee[^a]\n");
    assert!(
        !html.contains("<blockquote>"),
        "a tab-indented quote opened a real block:\n{html}"
    );
    assert!(
        html.contains("&gt; q"),
        "the quote marker is not literal text:\n{html}"
    );
}

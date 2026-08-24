//! PART 11 §2 over a bracketed label: `[^x]` is a note reference, so a span or
//! a link whose CONTENT begins with a caret has to escape it.
//!
//! §2's test names the RE-PARSED SOURCE, and that is the operative word: it is
//! evaluated against the source the writer will emit, not against the tree the
//! writer emits from. The two differ, because a writer normalizes - a `span`
//! node says nothing about the bracket run it is about to be spelled with, and
//! the collision only exists once it has been.
//!
//! Measured for markup-carve/carve-rs#1311 against the shared fixture
//! `note-reference-in-a-span` (spec `82fe8050`, markup-carve/carve#1618).
//! carve-js escapes this in its canonical writer (`escapeNoteReferenceLabel`
//! in `src/render-carve.ts`, reached from both the span arm and `renderLink`);
//! carve-php escapes it in `HtmlToCarve::escapeNoteReferenceLabel`, because
//! that engine's importer writes source directly rather than through a writer.
//! This engine is AST-based like carve-js, so the escape belongs in the writer,
//! where every path that spells a bracket run reaches it: an HTML import, an
//! ingested AST and `fmt` over parsed source alike.
//!
//! §2 is a minimality requirement in BOTH directions, so the over-escape is a
//! defect of the same size as the missing one. That is why every assertion here
//! has its negative twin.

/// A `span` whose text is `^1`, spelled by the writer.
const LABELED_SPAN: &str = "<p><abbr title=\"y\">^1</abbr></p>";
/// The same shape with nothing after the caret, which opens no reference.
const BARE_SPAN: &str = "<p><abbr title=\"y\">^</abbr></p>";

fn imported(html: &str) -> String {
    carve::html_to_carve(html, &carve::HtmlImportOptions::default())
        .expect("import")
        .value
}

#[test]
fn the_premise_an_unescaped_caret_label_is_a_note_reference_and_not_a_span() {
    // Without this the whole question is idle. `[^1]{abbr=y}` has to actually
    // re-read as something other than the span, or §2 would FORBID the escape.
    let as_written = carve::to_html("[^1]{abbr=y}\n");
    assert!(
        !as_written.contains("abbr"),
        "expected the span to be gone, got {as_written:?}"
    );
    // And the escaped spelling has to bring it back, or the escape is the wrong
    // one rather than a missing one.
    let escaped = carve::to_html("[\\^1]{abbr=y}\n");
    assert!(
        escaped.contains("<abbr title=\"y\">"),
        "expected the escaped spelling to render the span, got {escaped:?}"
    );
}

#[test]
fn the_negative_premise_a_bare_caret_label_is_already_a_span() {
    // `[^]` is not a note reference - the rule needs at least one character
    // after the caret - so an escape here would be the idle mark §2 forbids.
    let as_written = carve::to_html("[^]{abbr=y}\n");
    assert!(
        as_written.contains("<abbr title=\"y\">"),
        "expected `[^]{{abbr=y}}` to already be a span, got {as_written:?}"
    );
}

#[test]
fn a_span_label_opening_a_note_reference_is_escaped_and_a_bare_caret_is_not() {
    assert_eq!(imported(LABELED_SPAN), "[\\^1]{abbr=y}\n");
    assert_eq!(imported(BARE_SPAN), "[^]{abbr=y}\n");
}

#[test]
fn the_escaped_spelling_is_a_writer_fixed_point() {
    // A mark that is not a fixed point grows one backslash per format pass.
    assert_eq!(carve::to_carve("[\\^1]{abbr=y}\n"), "[\\^1]{abbr=y}\n");
    assert_eq!(carve::to_carve("[^]{abbr=y}\n"), "[^]{abbr=y}\n");
}

#[test]
fn the_written_source_renders_what_the_html_held() {
    // The whole point of the escape, stated as the round trip rather than as a
    // string: the paragraph the HTML held comes back.
    for html in [LABELED_SPAN, BARE_SPAN] {
        let rendered = carve::to_html(&imported(html));
        assert!(
            rendered.contains("<abbr title=\"y\">"),
            "expected the span to survive {html:?}, got {rendered:?}"
        );
    }
}

#[test]
fn a_link_label_takes_the_same_escape() {
    // The label slot is the collision, not the span: `[^1](u)` re-reads as a
    // note reference followed by the literal characters `(u)`, so the anchor
    // loses its destination exactly the way the span loses its attributes.
    let written = imported("<p><a href=\"u\">^1</a></p>");
    assert_eq!(written, "[\\^1](u)\n");
    assert!(carve::to_html(&written).contains("href=\"u\""));
    // And the bare caret keeps no escape here either.
    assert_eq!(imported("<p><a href=\"u\">^</a></p>"), "[^](u)\n");
}

#[test]
fn a_caret_that_is_not_first_is_ordinary_punctuation() {
    // The rule is about the character that OPENS the label. One anywhere else
    // opens nothing, so escaping it would be idle.
    assert_eq!(
        imported("<p><abbr title=\"y\">a^1</abbr></p>"),
        "[a^1]{abbr=y}\n"
    );
}

#[test]
fn a_notes_own_content_recognizes_no_note_so_the_escape_is_not_spent_there() {
    // PART 9 §16: note recognition is disabled at every depth inside an inline
    // note's content, so the bracket run there is already read as what it is.
    // An escape would be the idle mark §2 forbids just as squarely as the
    // missing one.
    let written = carve::to_carve("a^[[^1]{abbr=y}]\n");
    assert!(
        !written.contains("[\\^1]"),
        "expected no escape inside a note's content, got {written:?}"
    );
    assert_eq!(
        carve::to_html(&written),
        carve::to_html("a^[[^1]{abbr=y}]\n")
    );
}

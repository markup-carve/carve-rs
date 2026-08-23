//! The writer escapes a `^` before a `[` only where the note can FORM.
//!
//! PART 11 §2 escapes a character IF AND ONLY IF omitting the escape would
//! change the re-parsed AST, and it takes the decision per OPENER OCCURRENCE.
//! The writer treated every `^` before a `[` as one. PART 9 §16 rules out two
//! shapes where it is not: an empty or whitespace-only body is literal, and note
//! recognition is DISABLED inside a note's own content, at every depth. Two more
//! fall out of asking the question at all: an unterminated `^[` opens nothing -
//! the same reading PART 11 §2a already records for `[^` - and a `[` reported by
//! a CODE SPAN is not adjacent to the caret, because the span writes its
//! backtick first. That last one is a distinction `boundary_text` does not draw:
//! it names the adjacent CHARACTER for the emphasis and comment-spacing
//! decisions, and only three node kinds write their own value at byte zero.
//!
//! So `x ^[]` came back `x \^[]`, `x ^[a ^[b] c]` came back `x ^[a \^[b] c]`,
//! `x ^[a` came back `x \^[a` and ``x ^`[t]` `` came back ``x \^`[t]` ``. All
//! four re-parse to the document they started from, which is what makes this
//! over-escaping rather than a rendering bug, and it is why nothing here could
//! see it: the corpus gate reads committed fixtures and none of these has one.
//!
//! ## The controls are on the ingest path, deliberately
//!
//! A parse cannot hand the writer a bare text `^` in front of a bracket run that
//! WOULD open a note: the parser reaches that `^` first and builds the note, so
//! the character only survives as text where the note did not form - or as an
//! `escaped_text` node, which the writer emits through its own branch and never
//! asks this question about. Controls written as Carve source would therefore
//! pass with the decision deleted, which is no control at all.
//!
//! PART 12 ingest is where the shape is real, so that is where they are written.

use carve::{from_json, render_carve, to_carve, to_html};

/// A document holding one paragraph with these inline nodes, as PART 12 JSON.
fn ingested(inline_json: &str) -> String {
    let json = format!(
        r#"{{"type":"document","children":[{{"type":"paragraph","children":[{inline_json}]}}],"srcByteLength":0}}"#
    );
    let doc = from_json(&json).expect("decode AST");
    render_carve(&doc).expect("write")
}

/// The two PART 11 §1 invariants, over the bytes the writer produced.
fn invariants_hold(source: &str) {
    let once = to_carve(source);
    assert_eq!(
        to_carve(&once),
        once,
        "fmt(fmt(x)) != fmt(x) for {source:?}: {once:?}"
    );
    assert_eq!(
        to_html(&once),
        to_html(source),
        "to_html(fmt(x)) != to_html(x) for {source:?}"
    );
}

#[test]
fn a_caret_before_a_bracket_that_opens_no_note_is_written_bare() {
    let cases = [
        // corpus 307-an-empty-inline-note-is-literal and -2: an empty or
        // whitespace-only body is literal (PART 9 §16).
        "x ^[]\n",
        "x ^[ ]\n",
        // corpus 309-a-note-s-content-recognizes-no-note and -3: recognition is
        // off for the whole content, so depth two is literal for the same
        // reason.
        "x ^[a ^[b] c]\n",
        "x ^[a ^[b ^[c] d] e]\n",
        // An unterminated run opens nothing. PART 11 §2a states this for `[^`.
        "x ^[a\n",
        "x ^[a b\n",
        // The `[` belongs to a code span, which writes its backtick first, so
        // the caret is not in front of a bracket in the output at all.
        "x ^`[t]` y\n",
        // A note that is only a note because of what follows the brackets.
        "x ^[](/u)\n",
        "x ^[]{.c}\n",
    ];
    for source in cases {
        assert_eq!(to_carve(source), source, "over-escaped {source:?}");
        invariants_hold(source);
    }
}

#[test]
fn a_caret_that_would_open_a_note_keeps_its_escape() {
    // The run is right here and its body is not blank, so writing the caret bare
    // would turn a paragraph into a note.
    assert_eq!(
        ingested(r#"{"type":"text","value":"x ^[note]"}"#),
        "x \\^[note]\n"
    );
    // The run starts in the NEXT node. Three node kinds write their own value
    // at byte zero and so can put a `[` against the caret this way; the rest
    // write a sigil, a backtick or a backslash first and cannot.
    for next in [
        r#"{"type":"text","value":"[note]"}"#,
        r#"{"type":"smart_punctuation","kind":"right_double_quote","value":"[note]"}"#,
        r#"{"type":"abbreviation","abbr":"[note]","expansion":"e"}"#,
    ] {
        assert_eq!(
            ingested(&format!(r#"{{"type":"text","value":"x ^"}},{next}"#)),
            "x \\^[note]\n",
            "escape dropped before {next}"
        );
    }
    // ... and the ones that cannot are written bare, which is the shape a
    // source document reaches: a code span reports a `[` as the adjacent
    // character while writing a backtick ahead of it.
    assert_eq!(
        ingested(r#"{"type":"text","value":"x ^"},{"type":"code","value":"[note]"}"#),
        "x ^`[note]`\n"
    );
    // The caret can also arrive from an ABBREVIATION node, which writes its own
    // text into the same run. That arm passed `\0` for both neighbours, so the
    // decision was told there was nothing beside it.
    assert_eq!(
        ingested(
            r#"{"type":"abbreviation","abbr":"x ^","expansion":"e"},{"type":"smart_punctuation","kind":"right_double_quote","value":"[note]"}"#
        ),
        "x \\^[note]\n"
    );
    // The `]` arrives past a node the writer has not rendered yet, so this one
    // is not decided here at all: the minimal form opens a note, the two passes
    // disagree, and PART 11 §4's vote picks between them.
    //
    // THIS ASSERTION READ `x \^\[a *b* \]` UNTIL PART 11 §2b, and the reason
    // it did is the reason §2b exists. §4's vote took the conservative form for
    // the WHOLE DOCUMENT, so every candidate in every run was escaped once one
    // of them had to be. §2b bounds the fallback to the smallest unit that
    // fails, and here that is the LAST run: neutralizing the `]` is enough to
    // stop the note forming, so the first run keeps its `^` and `[` bare. One
    // escape where there were three, and the written form still re-parses to
    // this tree, which is what the vote actually asks.
    //
    // The caret decision itself is untouched - it is still "not decided here",
    // and it is still the vote that settles it. What changed is how far the
    // vote's answer reaches. carve-js and carve-php write the same bytes for
    // this tree (markup-carve/carve-js#1307, markup-carve/carve-php#1560).
    assert_eq!(
        ingested(
            r#"{"type":"text","value":"x ^[a "},{"type":"strong","children":[{"type":"text","value":"b"}]},{"type":"text","value":" ]"}"#
        ),
        "x ^[a *b* \\]\n"
    );
}

#[test]
fn an_ingested_bracket_run_is_neutralized_without_the_caret() {
    // An empty body cannot open a note whatever follows the brackets - the
    // reader requires a non-blank body before it looks at a tail at all - so the
    // caret is not the character that has to move here. The OPENING BRACKET is,
    // and it is a PART 11 §5 candidate, which is the vote's own job: the
    // minimal form re-parses as a link (or a span), the two passes disagree,
    // and the conservative form goes out for the run that failed.
    //
    // ONE BRACKET, not the whole run. §2 decides per opener occurrence
    // (markup-carve/carve#1533), and a link opens on its `[`: suppress that one
    // and the rest is ordinary text. The unit-scoped form escaped every
    // candidate in the run, and all but the first backslash were idle.
    //
    // Written on the ingest path because that is the only way a text node can
    // hold this spelling: parsed from source, the `[](/u)` is already a link
    // node and the writer emits it as one.
    for (inline, expected) in [
        (
            r#"{"type":"text","value":"x ^[](/u) y"}"#,
            "x ^\\[](/u) y\n",
        ),
        (
            r#"{"type":"text","value":"x ^[]{.c} y"}"#,
            "x ^\\[]{.c} y\n",
        ),
        (r#"{"type":"text","value":"x ^[] y"}"#, "x ^[] y\n"),
    ] {
        assert_eq!(ingested(inline), expected, "moved for {inline}");
    }
}

#[test]
fn an_authored_escape_is_written_back() {
    // These reach the writer as `escaped_text` nodes rather than as text, so
    // they do not exercise the decision above - they pin that it did not start
    // dropping an escape the author wrote.
    for source in [
        "x \\^[note]\n",
        "x \\^[t](/u)\n",
        "x \\^[a *b* ]\n",
        "x \\^[a `]` b]\n",
    ] {
        assert_eq!(to_carve(source), source, "control moved for {source:?}");
        invariants_hold(source);
    }
}

#[test]
fn a_caret_a_brace_binds_keeps_its_escape() {
    // The other arm of the same decision. `{^x^}` itself is a superscript node
    // and never reaches this code, so it is not the shape to write here: these
    // three are, because the caret arrives as ordinary text beside a brace and
    // writing it bare would let a superscript form.
    for (source, expected) in [
        ("a {^x b\n", "a {\\^x b\n"),
        ("a x^} b\n", "a x\\^} b\n"),
        ("a {^ b\n", "a {\\^ b\n"),
    ] {
        assert_eq!(to_carve(source), expected, "control moved for {source:?}");
        invariants_hold(source);
    }
}

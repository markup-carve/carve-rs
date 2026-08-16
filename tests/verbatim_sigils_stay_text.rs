//! Text that ENDS on a `$`, `$$` or `!` in front of a verbatim span is escaped.
//!
//! Each of those sigils binds to the backtick fence the next node writes: `$`
//! makes the span inline math, `$$` display math, `!` an inline literal. So a
//! Markdown paragraph reading `a $` then a code span came out of the migration
//! as `` a $`x+y` b ``, which is inline math - a construct the source never
//! spelled, produced with no diagnostic. PART 11 §2 escapes a character IF AND
//! ONLY IF omitting the escape would change the re-parsed AST, and omitting it
//! here does.
//!
//! Corpus-convert `05-markdown-verbatim-sigils-stay-text` is the document that
//! asks, and the ruling behind it is carve#1130: CommonMark plus GFM is the
//! contract, so a construct only Carve spells is escaped and the migrated
//! document keeps saying what its author wrote. carve-js escapes it in
//! `escapeCarveConstructsSpelledLikeText` and carve-php in its Markdown
//! converter, both line rewriters. carve-rs's importers are AST-first and hand
//! the source-writing job to the canonical writer, so the rule lives there - and
//! living there covers every importer at once instead of one.
//!
//! ## Why the controls are on the ingest and import paths
//!
//! A parse cannot hand the writer a bare text `$` in front of a code span: the
//! parser reaches the `$` first and builds the math node, so the character only
//! survives as text where no construct formed, or as an `escaped_text` node the
//! writer emits through a different branch and never asks this question about.
//! Controls written as Carve source would therefore still pass with the whole
//! decision deleted, which is no control at all. PART 12 ingest and the
//! importers are where the shape is real.
//!
//! ## One shape this rule does not reach, deliberately
//!
//! The question is asked of the NEXT NODE, so a node between the sigil and the
//! span that emits no bytes hides the fence: ingested text `a $`, an EMPTY text
//! node and a code span still write `` a $`x+y` ``. No parser builds that - two
//! adjacent text runs merge - so it is reachable through PART 12 ingest only,
//! and carve-js and carve-php write the same bytes for it, measured. It is
//! therefore a cross-engine question rather than a carve-rs divergence, and
//! answering it here alone would introduce one. The neighbouring
//! `next_node_opens_a_note` asks its question the same way for the same reason.

use carve::{from_json, markdown_to_carve, render_carve, to_carve, to_html};

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
fn a_markdown_document_keeps_its_sigils_as_text() {
    let migrated = markdown_to_carve("a $`x+y` b\n\nc $$`x+y` d\n\ne !`x` f\n");
    assert_eq!(migrated, "a \\$`x+y` b\n\nc \\$\\$`x+y` d\n\ne \\!`x` f\n");
    // The bytes carve-js and carve-php write for the same input, and the render
    // the corpus case pins.
    assert_eq!(
        to_html(&migrated),
        "<p>a $<code>x+y</code> b</p>\n<p>c $$<code>x+y</code> d</p>\n<p>e !<code>x</code> f</p>"
    );
}

#[test]
fn an_html_document_keeps_its_sigils_as_text() {
    // The same shape reaches the writer from the HTML importer, which is the
    // point of fixing it in the writer rather than in one converter.
    let migrated = carve::html_to_carve(
        "<p>a $<code>x+y</code> b</p>",
        &carve::HtmlImportOptions::default(),
    )
    .expect("import")
    .value;
    assert_eq!(migrated, "a \\$`x+y` b\n");
    assert_eq!(to_html(&migrated), "<p>a $<code>x+y</code> b</p>");
}

#[test]
fn an_ingested_sigil_before_a_verbatim_span_is_escaped() {
    // EVERY dollar of the run, not just the last: in `\$$`x`` the second dollar
    // still opens inline math.
    assert_eq!(
        ingested(r#"{"type":"text","value":"a $"},{"type":"code","value":"x+y"}"#),
        "a \\$`x+y`\n"
    );
    assert_eq!(
        ingested(r#"{"type":"text","value":"c $$"},{"type":"code","value":"x+y"}"#),
        "c \\$\\$`x+y`\n"
    );
    assert_eq!(
        ingested(r#"{"type":"text","value":"e !"},{"type":"code","value":"x"}"#),
        "e \\!`x`\n"
    );
    // A raw inline writes the same fence at byte zero, so the sigil binds to it
    // the same way.
    assert_eq!(
        ingested(
            r#"{"type":"text","value":"a $"},{"type":"raw_inline","format":"html","content":"<b>"}"#
        ),
        "a \\$`<b>`{=html}\n"
    );
    for source in [
        "a \\$`x+y` b\n",
        "c \\$\\$`x+y` d\n",
        "e \\!`x` f\n",
        "a \\$`<b>`{=html}\n",
    ] {
        invariants_hold(source);
    }
}

#[test]
fn a_sigil_that_binds_to_nothing_keeps_no_escape() {
    // THE CONTROLS. Each of these would pick up an escape from a rule that
    // escaped the class of character rather than the occurrence, and each
    // re-parses to the document it started from without one - which is the
    // over-escaping PART 11 §2 calls a defect rather than a safe default.
    //
    // Nothing follows the sigil at all.
    assert_eq!(ingested(r#"{"type":"text","value":"a $ b"}"#), "a $ b\n");
    assert_eq!(ingested(r#"{"type":"text","value":"5 $"}"#), "5 $\n");
    assert_eq!(ingested(r#"{"type":"text","value":"wow!"}"#), "wow!\n");
    // A space separates the sigil from the fence, so the run is not adjacent.
    assert_eq!(
        ingested(r#"{"type":"text","value":"a $ "},{"type":"code","value":"x"}"#),
        "a $ `x`\n"
    );
    // The sigil is not at the end of the node, so the fence is not what follows
    // it.
    assert_eq!(
        ingested(r#"{"type":"text","value":"a $b"},{"type":"code","value":"x"}"#),
        "a $b`x`\n"
    );
    // The NEXT node writes its own sigil ahead of the fence, so the text's
    // sigil is not against a backtick either: `$!` opens nothing.
    assert_eq!(
        ingested(r#"{"type":"text","value":"a $"},{"type":"literal_inline","content":"x"}"#),
        "a $!`x`\n"
    );
    // A math node ahead of the text's own `$` is the same branch and a
    // DIFFERENT question: all three engines write `a $$`x`` there, which
    // re-parses as display math, so it is a cross-engine round-trip issue of
    // its own rather than something this rule reaches. It is left unasserted
    // here so a fix for it does not have to argue with this file.
    for source in ["a $ b\n", "5 $\n", "wow!\n", "a $ `x` b\n", "a $b`x` c\n"] {
        assert_eq!(to_carve(source), source, "over-escaped {source:?}");
        invariants_hold(source);
    }
}

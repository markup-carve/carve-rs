//! The no-break-space sentinel U+E000 is resolved on `code_block.content`
//! (PART 12 §3, markup-carve/carve#1262).
//!
//! The sentinel rides on FOUR fields - `text.value`, `code.value`,
//! `code_block.content` and `literal_inline.content` - and the clause is the
//! same on all four: a consumer MUST map U+E000 to its target's no-break space,
//! or to an ordinary space where the target has none, and MUST NOT emit it.
//! carve-rs mapped three of them and passed the private-use character straight
//! into `<pre><code>`; the Markdown target mapped only `text.value`.
//!
//! A private-use codepoint reaching a consumer is not invisible. A typesetter
//! draws its font's `.notdef` glyph for it - a visible box, wider than the
//! space it stands for, with no warning emitted.
//!
//! TWO SOURCES put the sentinel there and both are covered below, separately:
//!
//! - an escaped space (`\ `), which is ONE sentinel;
//! - a line block's preserved indentation (PART 9 §23), which is a RUN of one
//!   sentinel per space. A fix that only knew about the escaped space maps the
//!   first and leaves the rest, so the run gets its own assertions.
//!
//! `raw_block.content` is deliberately EXCLUDED from the rule - it is
//! byte-for-byte passthrough - and is asserted here as a CONTROL: a fix that
//! resolves everywhere is as wrong as one that resolves nowhere.

use carve::{
    from_json, render_ansi, render_carve, render_html, render_markdown, render_plain_text,
};

/// The no-break-space sentinel.
const NBSP: char = '\u{e000}';

/// The escaped-space shape: one sentinel standing for one `\ `.
const ONE: &str = "gap\u{e000}here";

/// The line-block shape: a RUN of four, the indentation of a four-space verse
/// line (PART 9 §23), one sentinel per preserved space.
const RUN: &str = "\u{e000}\u{e000}\u{e000}\u{e000}indented verse";

/// A document carrying the sentinel on `code_block.content` from both sources,
/// plus a `raw_block` carrying it as payload.
fn wire_doc() -> carve::Document {
    let json = format!(
        r#"{{"type":"document","srcByteLength":0,"children":[
            {{"type":"code_block","content":"{ONE}\n{RUN}"}},
            {{"type":"raw_block","format":"html","content":"<i>{NBSP}raw</i>"}}
        ]}}"#
    );
    from_json(&json).expect("the wire document decodes")
}

fn html() -> String {
    render_html(&wire_doc()).expect("html renders")
}

fn markdown() -> String {
    render_markdown(&wire_doc()).expect("markdown renders")
}

#[test]
fn an_escaped_space_in_a_code_block_renders_as_nbsp_in_html() {
    let out = html();
    assert!(
        out.contains("gap&nbsp;here"),
        "the escaped-space sentinel was not mapped in <pre><code>: {out:?}"
    );
}

#[test]
fn a_line_block_indent_run_in_a_code_block_renders_one_nbsp_per_space_in_html() {
    let out = html();
    assert!(
        out.contains("&nbsp;&nbsp;&nbsp;&nbsp;indented verse"),
        "the four-sentinel verse indent was not mapped one for one: {out:?}"
    );
}

#[test]
fn no_sentinel_survives_into_the_code_block_html() {
    let out = html();
    // The raw block keeps its own, so count the `<pre>` element alone.
    let code = out
        .split_once("<pre>")
        .and_then(|(_, rest)| rest.split_once("</pre>"))
        .map(|(inner, _)| inner.to_string())
        .expect("the code block is in the output");
    assert!(
        !code.contains(NBSP),
        "a private-use character reached the rendered code block: {code:?}"
    );
}

#[test]
fn an_authored_sentinel_in_a_code_block_is_mapped_from_source_too() {
    // Not only the wire path: a U+E000 the author typed into a fence is the
    // same character under the same rule.
    let src = format!("```\nblock{NBSP}w\n```\n");
    let out = carve::to_html(&src);
    assert!(
        out.contains("block&nbsp;w"),
        "an authored sentinel was emitted raw: {out:?}"
    );
    assert!(!out.contains(NBSP), "the sentinel survived: {out:?}");
}

#[test]
fn a_raw_blocks_sentinel_is_left_alone_in_html() {
    // CONTROL. `raw_block.content` is byte-for-byte passthrough, so its U+E000
    // is payload the consumer must not touch. Mapping it would corrupt exactly
    // what the node exists to carry.
    let out = html();
    assert!(
        out.contains(&format!("<i>{NBSP}raw</i>")),
        "the raw block's sentinel was rewritten: {out:?}"
    );
}

#[test]
fn both_sources_map_to_a_no_break_space_in_markdown() {
    let out = markdown();
    assert!(
        out.contains("gap\u{00a0}here"),
        "the escaped-space sentinel was not mapped in Markdown: {out:?}"
    );
    assert!(
        out.contains("\u{00a0}\u{00a0}\u{00a0}\u{00a0}indented verse"),
        "the verse indent run was not mapped in Markdown: {out:?}"
    );
    assert!(
        out.contains(&format!("<i>{NBSP}raw</i>"))
            || out.contains(&format!("&lt;i&gt;{NBSP}raw&lt;/i&gt;")),
        "CONTROL: the raw block's sentinel was rewritten in Markdown: {out:?}"
    );
}

#[test]
fn a_code_span_and_an_inline_literal_map_in_markdown() {
    // The Markdown target resolved `text.value` alone, so these two leaked the
    // character into a fenced-code span and into prose.
    let src = format!("`code{NBSP}y` and !`lit{NBSP}z`\n");
    let out = carve::to_markdown(&src);
    assert!(
        out.contains("code\u{00a0}y"),
        "code.value was not mapped in Markdown: {out:?}"
    );
    assert!(
        out.contains("lit\u{00a0}z"),
        "literal_inline.content was not mapped in Markdown: {out:?}"
    );
}

#[test]
fn plain_and_ansi_fall_back_to_an_ordinary_space() {
    // Neither target has a no-break space, so the rule's second half applies.
    // Both already did this; the assertion keeps them from drifting.
    let doc = wire_doc();
    for (label, out) in [
        ("plain", render_plain_text(&doc).expect("plain renders")),
        ("ansi", render_ansi(&doc).expect("ansi renders")),
    ] {
        assert!(
            !out.contains(NBSP),
            "the sentinel reached the {label} target: {out:?}"
        );
        assert!(
            out.contains("gap here"),
            "the {label} target lost the space the sentinel stood for: {out:?}"
        );
    }
}

#[test]
fn the_canonical_carve_writer_keeps_the_sentinel() {
    // CONTROL, and deliberately NOT the rule above. The canonical writer's
    // target is Carve source, where a code block is verbatim and has no escape
    // to spell a no-break space with. Keeping the character is what makes the
    // round trip lossless, and carve-js and carve-php keep it here too.
    let out = render_carve(&wire_doc()).expect("carve renders");
    assert!(
        out.contains(RUN),
        "the writer rewrote a code block it has to give back verbatim: {out:?}"
    );
}

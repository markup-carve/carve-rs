//! The writer does not escape into a run the reader reads RAW.
//!
//! An image's alt text is an HTML attribute: nothing inside it is inline-parsed
//! and no escape inside it is resolved, so `![t\]z](/i.png)` gives `alt="t\]z"`,
//! backslash and all. The writer escaped every `[`, `]` and `\` in it anyway, on
//! the premise that the run stops at the first `]` - the premise
//! markup-carve/carve#1206 removed from the grammar. So `![t[z]](/i.png)`, whose
//! alt is `t[z]`, came back written `t\[z\]` and the document said something
//! else, and it compounded: each pass escaped the backslash the last one wrote,
//! so `fmt(fmt(x)) == fmt(x)` failed from the second pass on.
//!
//! ## The same rule was written five times
//!
//! Every writer putting a value between brackets the reader will read raw
//! carried the same escape:
//!
//! | site | construct |
//! | --- | --- |
//! | `escape_image_alt` | an image's alt text |
//! | `escape_bracket_text` | an admonition label, a div label, a code-fence label |
//! | `escape_footnote_label` | a footnote definition and every reference to it |
//!
//! The four beyond alt text are reachable from ordinary source and broke in the
//! same direction, only more quietly, because their run holds no bracket to draw
//! attention. A backslash in any of them grew one more backslash per pass:
//! `::: [a\b]` became `::: [a\\b]` and then `::: [a\\\\b]`. A div label is
//! rendered, so that document said something new each time; the other three
//! merely refused to settle.
//!
//! ## Idempotence is asserted separately, on purpose
//!
//! A single `to_html(fmt(x)) == to_html(x)` pass is what this defect survived
//! where it was cheapest to catch: the first pass over a value whose only
//! special character is a backslash is unchanged, and the second is where the
//! backslash starts eating itself. A code-fence label and a footnote id are not
//! rendered at all, so a round-trip assertion on those two holds however mangled
//! the label gets - they are read back out of the tree instead.

use carve::ast::{BlockNode, InlineNode};
use carve::{from_json, parse, render_carve, to_carve, to_html};

fn fmt(source: &str) -> String {
    to_carve(source)
}

/// `fmt(fmt(x)) == fmt(x)`, stated rather than inferred from a round trip.
fn settles(source: &str) {
    let once = fmt(source);
    assert_eq!(
        fmt(&once),
        once,
        "fmt(fmt(x)) != fmt(x) for {source:?}: {once:?}"
    );
}

/// `to_html(fmt(x)) == to_html(x)`.
fn renders_the_same(source: &str) {
    let once = fmt(source);
    assert_eq!(
        to_html(&once),
        to_html(source),
        "to_html(fmt(x)) != to_html(x) for {source:?}"
    );
}

/// A document holding one paragraph with these inline nodes, as PART 12 JSON.
fn ingested(inline_json: &str) -> String {
    let json = format!(
        r#"{{"type":"document","children":[{{"type":"paragraph","children":[{inline_json}]}}],"srcByteLength":0}}"#
    );
    render_carve(&from_json(&json).expect("decode AST")).expect("write")
}

/// The first code block's label, read out of the tree rather than out of HTML -
/// a fence label is not rendered anywhere.
fn code_fence_label(source: &str) -> Option<String> {
    parse(source)
        .children
        .into_iter()
        .find_map(|block| match block {
            BlockNode::CodeBlock(code) => code.label,
            _ => None,
        })
}

/// The id of the first footnote reference, for the same reason.
fn footnote_ref_id(source: &str) -> Option<String> {
    fn walk(nodes: Vec<InlineNode>) -> Option<String> {
        for node in nodes {
            if let InlineNode::Footnote(footnote) = node {
                return footnote.id;
            }
        }
        None
    }
    parse(source)
        .children
        .into_iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => walk(paragraph.children),
            _ => None,
        })
}

#[test]
fn an_alt_text_is_written_as_authored() {
    // The four documents markup-carve/carve#1206 added that failed the formatter
    // sweep, plus the balanced-nesting shape they generalize.
    let cases = [
        "a ![t[z]](/i.png) b\n",
        "a ![t\\]z](/i.png) b\n",
        "a ![t`]`z](/i.png) b\n",
        "a ![t{# ] #}z](/i.png) b\n",
        "a ![t[z[q]]](/i.png) b\n",
    ];
    for source in cases {
        assert_eq!(fmt(source), source, "escaped into a raw run: {source:?}");
        settles(source);
        renders_the_same(source);
    }
}

#[test]
fn a_flat_label_is_written_as_authored() {
    // A div label and an admonition label are RENDERED, so the growing backslash
    // changed what the document says on every pass.
    for source in [
        "::: [a\\b]\nx\n:::\n",
        "::: note [a\\b]\nx\n:::\n",
        "::: [a\\\\b]\nx\n:::\n",
    ] {
        assert_eq!(fmt(source), source, "escaped into a raw run: {source:?}");
        settles(source);
        renders_the_same(source);
    }

    // A code-fence label renders nothing, so the value is read out of the tree.
    let fence = "``` rust [a\\b]\nc\n```\n";
    assert_eq!(fmt(fence), fence);
    settles(fence);
    assert_eq!(code_fence_label(fence).as_deref(), Some("a\\b"));
    assert_eq!(code_fence_label(&fmt(fence)).as_deref(), Some("a\\b"));

    // A footnote id renders nothing either - the number does.
    let note = "x [^a\\b] y\n\n[^a\\b]: n\n";
    assert_eq!(fmt(note), note);
    settles(note);
    renders_the_same(note);
    assert_eq!(footnote_ref_id(&fmt(note)).as_deref(), Some("a\\b"));

    // An empty definition takes the `{empty}` sentinel and the same label rule.
    let empty = "x [^a\\b] y\n\n[^a\\b]: {empty}\n";
    assert_eq!(fmt(empty), empty);
    settles(empty);
}

#[test]
fn inline_content_between_brackets_still_gets_its_escape() {
    // The other direction, and the reason this is not one rule for every pair of
    // brackets: a link's text, a span's and an inline note's content are INLINE
    // CONTENT. The reader resolves an escape there, so the escape is the
    // spelling of the character and the writer owes it.
    let cases = [
        ("a [t\\]z](/u) b\n", "a [t\\]z](/u) b\n"),
        ("a [t\\]z]{.c} b\n", "a [t\\]z]{.c} b\n"),
        ("a ^[t\\]z] b\n", "a ^[t\\]z] b\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(fmt(source), expected, "escape dropped for {source:?}");
        settles(source);
        renders_the_same(source);
    }
}

#[test]
fn an_alt_with_no_carve_spelling_keeps_the_escape() {
    // A bare unbalanced `]`. `parse` cannot produce this alt; an ingested AST
    // can. Escaping is not a representation of the value either, but it keeps
    // the image a well-formed image instead of letting the stray `]` split the
    // line, and it settles - the escaped alt IS representable.
    let once = ingested(r#"{"type":"image","src":"/i.png","alt":"t]z"}"#);
    assert_eq!(once, "![t\\]z](/i.png)\n");
    assert_eq!(fmt(&once), once, "the fallback does not settle");

    // A backslash already in the value is not doubled, because the run closes.
    assert_eq!(
        ingested(r#"{"type":"image","src":"/i.png","alt":"t\\]z"}"#),
        "![t\\]z](/i.png)\n"
    );
}

#[test]
fn an_abbreviation_definition_keeps_its_escape() {
    // Deliberately NOT folded into the flat rule. The term reader is
    // `is_ascii_alphanumeric`, per PART 5's `(letter | digit)+`, so neither
    // character can reach it from a parse - a shared shape, not a shared rule.
    let json = r#"{"type":"document","children":[{"type":"abbreviation_def","abbr":"a]b","expansion":"e"}],"srcByteLength":0}"#;
    let written = render_carve(&from_json(json).expect("decode AST")).expect("write");
    assert_eq!(written, "*[a\\]b]: e\n");
}

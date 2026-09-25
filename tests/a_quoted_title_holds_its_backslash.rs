//! A quoted title is the source VERBATIM, so a backslash in it is content.
//!
//! `quoted_title = '"', {character - '"'}, '"'` and its normative note say
//! there is no escape mechanism inside the slot. The reader consumed a
//! backslash before any character anyway, and the writer doubled one back, so
//! an authored `\` was unreachable in every output and the pair only looked
//! stable because the two errors were each other's inverse
//! (markup-carve/carve-rs#1946).
//!
//! The assertion is the INVARIANT, not one pass's bytes: the first pass looks
//! right in the compounding direction, which is why the sibling ruling
//! markup-carve/carve-php#2397 asks for two passes compared against each other
//! rather than an expected string. Both directions are asserted, because the two
//! halves pull opposite ways - reading must not drop a backslash and writing
//! must not add one.

use carve::{parse, render_carve, to_carve, to_html, BlockNode, Document, RenderCarveError};

/// (name, source)
fn shapes() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "a code fence, backslash before a space",
            "```php \"a \\ b\"\nx\n```\n",
        ),
        (
            "a colon fence, backslash before a space",
            "::: note \"a \\ b\"\nx\n:::\n",
        ),
        (
            "a directive, backslash before a space",
            "::: toc \"a \\ b\"\nx\n:::\n",
        ),
        (
            "a code fence, an escaped pair",
            "```php \"a \\\\ b\"\nx\n```\n",
        ),
        (
            "a colon fence, an escaped pair",
            "::: note \"a \\\\ b\"\nx\n:::\n",
        ),
        (
            "a code fence, a trailing backslash",
            "```php \"a \\\"\nx\n```\n",
        ),
        (
            "a colon fence, escaped asterisks",
            "::: note \"A \\*b\\* c\"\nx\n:::\n",
        ),
        (
            "a code fence, escaped asterisks",
            "```php \"A \\*b\\* c\"\nx\n```\n",
        ),
        (
            "a title, a label and a backslash",
            "```php \"a \\ b\" [t]\nx\n```\n",
        ),
        // Controls: no backslash anywhere in the slot.
        ("a code fence with no backslash", "```php \"a b\"\nx\n```\n"),
        (
            "a colon fence with no backslash",
            "::: note \"a b\"\nx\n:::\n",
        ),
    ]
}

/// decode then encode gives back the authored bytes.
#[test]
fn a_quoted_title_survives_a_format_pass() {
    for (name, source) in shapes() {
        assert_eq!(to_carve(source), source, "{name}");
    }
}

/// encode then decode likewise: a second pass adds and removes nothing.
#[test]
fn a_second_format_pass_changes_nothing() {
    for (name, source) in shapes() {
        let once = to_carve(source);
        assert_eq!(to_carve(&once), once, "{name}");
    }
}

/// The rendered document is what a lost backslash actually costs, and a byte
/// assertion on the written line cannot see it.
#[test]
fn a_format_pass_keeps_the_rendered_title() {
    for (name, source) in shapes() {
        assert_eq!(to_html(&to_carve(source)), to_html(source), "{name}");
    }
}

/// The parse changed, not only the bytes: the unescape ran before the title's
/// inline parse, so an authored `\*` reached no target and the emphasis it was
/// escaping to prevent appeared instead.
#[test]
fn an_escaped_asterisk_in_a_title_is_a_literal_asterisk() {
    let html = to_html("::: toc \"A \\*b\\* c\"\n:::\n");
    assert!(html.contains("A *b* c"), "{html}");
    assert!(!html.contains("<strong>"), "{html}");
}

/// The same run in a paragraph was already right, which is what places the
/// defect in the title slot alone.
#[test]
fn the_paragraph_control_reads_the_same_run_the_same_way() {
    assert!(to_html("A \\*b\\* c\n").contains("A *b* c"));
}

/// A backslash before a space is the inline non-breaking-space escape, so a
/// colon-fence title holds one where a code-fence title, which is raw text,
/// holds the backslash itself.
#[test]
fn the_two_fence_kinds_read_the_slot_by_their_own_content_model() {
    assert!(to_html("::: note \"a \\ b\"\nx\n:::\n").contains("a &nbsp;b"));
    assert!(to_html("```php \"a \\ b\"\nx\n```\n").contains("title=\"a \\ b\""));
}

/// An escaped pair keeps exactly one backslash through two passes - it does not
/// grow, which is markup-carve/carve-php#2397's direction, and does not shrink,
/// which is this repo's.
#[test]
fn an_escaped_pair_keeps_exactly_one_backslash_through_two_passes() {
    for source in [
        "```php \"a \\\\ b\"\nx\n```\n",
        "::: note \"a \\\\ b\"\nx\n:::\n",
    ] {
        let once = to_carve(source);
        let twice = to_carve(&once);
        assert_eq!(once.matches('\\').count(), 2, "{once}");
        assert_eq!(twice, once);
    }
}

/// The first `"` closes the slot, so an escaped quote leaves a remainder that is
/// neither a label nor whitespace and the line is an ordinary paragraph. The
/// backslash-skipping scan swallowed the closer and opened a container instead.
#[test]
fn an_escaped_quote_opens_no_container() {
    for source in [
        "::: toc \"A \\\"q\\\" c\"\n:::\n",
        "::: note \"A \\\"q\\\" c\"\nx\n:::\n",
        "```php \"A \\\"q\\\" c\"\nx\n```\n",
    ] {
        let html = to_html(source);
        assert!(html.starts_with("<p>"), "{source} -> {html}");
        // The colon spellings are bytewise stable; the code spelling falls back
        // to an inline code span, which the writer respells with the shortest
        // fence that holds it, so the invariant there is the render.
        assert_eq!(to_html(&to_carve(source)), html, "{source}");
    }
    assert_eq!(
        to_carve("::: note \"A \\\"q\\\" c\"\nx\n:::\n"),
        "::: note \"A \\\"q\\\" c\"\nx\n:::\n"
    );
}

/// Control: an unescaped quote was already a paragraph, and stays one.
#[test]
fn a_bare_quote_in_the_slot_is_still_a_paragraph() {
    let source = "::: note \"a \" b\"\nx\n:::\n";
    assert!(to_html(source).starts_with("<p>"));
    assert_eq!(to_carve(source), source);
}

fn titled(kind: &str, title: &str) -> Document {
    let mut doc = parse(&format!("::: {kind} \"t\"\nx\n:::\n"));
    match &mut doc.children[0] {
        BlockNode::Admonition(node) => set_text(node.title.as_deref_mut(), title),
        BlockNode::Directive(node) => set_text(node.title.as_deref_mut(), title),
        other => panic!("expected a container, got {other:?}"),
    }
    doc
}

fn set_text(title: Option<&mut [carve::InlineNode]>, value: &str) {
    let Some([carve::InlineNode::Text(text)]) = title else {
        panic!("expected a one-text title");
    };
    text.value = value.to_string();
}

/// A `"` is unspellable in the slot rather than escapable, which is
/// markup-carve/carve-php#2375's reading of it. A title carrying one - which only
/// an ingest can produce - is refused rather than written as `\"`, source no
/// conforming parser reads back.
#[test]
fn a_quote_in_a_title_is_unspellable() {
    for (kind, node_type) in [("note", "admonition"), ("toc", "directive")] {
        let error = render_carve(&titled(kind, "a \" b")).expect_err(kind);
        let RenderCarveError::SourceUnspellable(error) = error else {
            panic!("expected an unspellable refusal for {kind}");
        };
        assert_eq!(error.node_type(), node_type);
    }
    let mut doc = parse("```php \"t\"\nx\n```\n");
    let BlockNode::CodeBlock(code) = &mut doc.children[0] else {
        panic!("expected a code block");
    };
    code.title = Some("a \" b".to_string());
    let error = render_carve(&doc).expect_err("a code fence title");
    let RenderCarveError::SourceUnspellable(error) = error else {
        panic!("expected an unspellable refusal");
    };
    assert_eq!(error.node_type(), "code_block");
}

/// Control: a backslash in the same slot is NOT unspellable, so the refusal
/// above answers the quote and not the escaping.
#[test]
fn a_backslash_in_a_title_is_spellable() {
    for kind in ["note", "toc"] {
        assert!(render_carve(&titled(kind, "a \\ b")).is_ok(), "{kind}");
    }
}

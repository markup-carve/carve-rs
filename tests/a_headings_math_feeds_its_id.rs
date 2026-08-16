//! Math is VISIBLE VERBATIM TEXT, so it feeds a heading's derived text and
//! therefore its id (markup-carve/carve#1283).
//!
//! `InlineNode::Math` had no arm in any of the three flatteners, so the trailing
//! `_ => {}` swallowed it: `# a $`x` b` published `id="a-b"` and a heading that
//! was ONLY math fell all the way through to the empty-text fallback `s`.
//!
//! MEASURED, NOT ASSUMED, before the arms were added: carve-js's `inlineText`
//! carries `case 'math': out += n.content` - grouped in the same arm as
//! `literal_inline` - and carve-php agrees, so carve-rs was the sole outlier.
//! The deciding asymmetry is not a head count: a code span and a math span are
//! the same shape of node, both holding verbatim text, and carve-rs contributed
//! one and dropped the other.
//!
//! THREE ARMS MOVE TOGETHER, the precedent carve-rs#800 set for the escaped-text
//! arm: `render::plain_inlines_typography` (HTML),
//! `parse::plain_inlines_parse` (the parse-time index) and
//! `render_markdown::plain_inlines` (the Markdown target) are three spellings of
//! ONE derivation, and a heading id derived three ways has to be one id.
//!
//! Each arm was MEASURED by removing it from the committed fix and running this
//! binary, rather than added on principle:
//!
//! - `render::plain_inlines_typography` is the arm the id is assigned THROUGH.
//!   Without it six of the eight cases below go red.
//! - `parse::plain_inlines_parse` feeds PART 9R R1's `by_text` index and nothing
//!   else here. Without it exactly ONE case goes red, and it is the interesting
//!   one: the heading still rendered `<section id="a-x-b">` while
//!   `[a $`x` b][]` resolved through the untouched parse-time key to `a-b`, so a
//!   render-only fix would have SHIPPED A NEW BUG - a link pointing at an anchor
//!   no element in the document carried.
//! - `render_markdown::plain_inlines` is a GREEN MUTATION, diagnosed rather than
//!   left. The Markdown writer emits the id resolution already put on the
//!   heading, so its flattener is never consulted for the `{#id}` it writes;
//!   `# $`x`` twice plus two references produces byte-identical Markdown with
//!   and without the arm. It still feeds the writer's own dedup COUNTER, whose
//!   key has to agree with the core's for the suffixes to line up the moment a
//!   heading reaches it without a resolved id. That it is currently unpinned is
//!   the finding, and this comment is where it is recorded rather than an
//!   assertion pretending to hold it.

/// The id MOVES: math contributes its content, so the span between the words
/// stops being a hole in the slug. This is the public-surface change.
#[test]
fn math_contributes_its_text_to_the_heading_id() {
    for (src, id) in [
        ("# a $`x` b\n", "a-x-b"),
        ("# a $$`x` b\n", "a-x-b"),
        ("# $`x`\n", "x"),
        ("# $$`x`\n", "x"),
    ] {
        let out = carve::to_html(src);
        assert!(
            out.contains(&format!("<section id=\"{id}\">")),
            "{src:?}: {out}"
        );
    }
}

/// A heading that is ONLY math no longer takes the empty-text fallback. Pinned
/// separately from the moved id above because it is a different code path: the
/// fallback fires on an empty derivation, not on a different one.
#[test]
fn a_math_only_heading_no_longer_takes_the_empty_text_fallback() {
    let out = carve::to_html("# $`x`\n");
    assert!(out.contains("<section id=\"x\">"), "{out}");
    assert!(!out.contains("id=\"s\""), "{out}");
}

/// CONTROL, and the one that stops an over-broad change: a heading with NO math
/// derives exactly the id it derived before. If a rewrite of the flattener moved
/// any of these, the fix reached further than the ticket.
#[test]
fn control_a_heading_without_math_derives_the_same_id() {
    for (src, id) in [
        ("# a `c` b\n", "a-c-b"),
        ("# a b\n", "a-b"),
        ("# a *b* c\n", "a-b-c"),
        ("# a [l](u) b\n", "a-l-b"),
        ("# a ![alt](i.png) b\n", "a-alt-b"),
        ("# a </#a-b> b\n", "a-b"),
    ] {
        let out = carve::to_html(src);
        assert!(
            out.contains(&format!("<section id=\"{id}\">")),
            "{src:?}: {out}"
        );
    }
}

/// CONTROL: the math still RENDERS as math. Feeding the derivation must not turn
/// the node into prose on the way through.
#[test]
fn control_the_math_still_renders_as_math() {
    let out = carve::to_html("# a $`x` b\n");
    assert!(
        out.contains("<span class=\"math inline\">\\(x\\)</span>"),
        "{out}"
    );
    let display = carve::to_html("# a $$`x` b\n");
    assert!(
        display.contains("<span class=\"math display\">\\[x\\]</span>"),
        "{display}"
    );
}

/// A cross-reference resolves against the id the heading publishes, so the two
/// have to agree once the id moves.
#[test]
fn a_crossref_resolves_to_the_moved_id() {
    let out = carve::to_html("# a $`x` b\n\nSee </#a-x-b>\n");
    assert!(out.contains("<section id=\"a-x-b\">"), "{out}");
    assert!(out.contains("href=\"#a-x-b\""), "{out}");
}

/// PART 9R R1's implicit `[label][]` index is keyed by the heading's derived
/// TEXT, and it is derived by the PARSE-time flattener. This is the assertion
/// that fails if only the render-time arm is added - the dangling anchor
/// described in the module comment.
#[test]
fn a_by_text_reference_resolves_to_the_moved_id() {
    let out = carve::to_html("# a $`x` b\n\nSee [a $`x` b][]\n");
    assert!(out.contains("<section id=\"a-x-b\">"), "{out}");
    assert!(
        out.contains("href=\"#a-x-b\""),
        "the by_text index must key on the same derivation the id came from: {out}"
    );
    assert!(
        !out.contains("href=\"#a-b\""),
        "a link to an anchor no element carries: {out}"
    );
}

/// Two math-only headings dedup in the shared document-order namespace like any
/// other repeated slug, rather than colliding on the old fallback.
#[test]
fn two_math_only_headings_dedup_on_the_derived_id() {
    let out = carve::to_html("# $`x`\n\n# $`x`\n");
    assert!(out.contains("<section id=\"x\">"), "{out}");
    assert!(out.contains("<section id=\"x-2\">"), "{out}");
}

/// The Markdown target is the third flattener, and it agrees.
#[test]
fn the_markdown_target_derives_the_same_id() {
    let doc = carve::parse("# a $`x` b\n\nSee </#a-x-b>\n");
    let md = carve::render_markdown(&doc).expect("markdown renders");
    assert!(md.contains("(#a-x-b)"), "{md}");
}

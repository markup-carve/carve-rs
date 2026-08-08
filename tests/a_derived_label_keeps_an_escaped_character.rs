//! An escaped character is VISIBLE PROSE, so it feeds every text derived from
//! the run it sits in (carve-rs#800).
//!
//! `plain_inlines_parse` is the single derivation behind a heading's title, its
//! generated id and PART 9R R1's `by_text` index; `plain_inlines` is the
//! render-time spelling of the same derivation. Neither had an arm for
//! `InlineNode::EscapedText`, and the trailing `_ => {}` swallowed it, so a
//! character the author deliberately escaped contributed NOTHING - not to the
//! key, not to the title, not to the id.
//!
//! MEASURED, NOT ASSUMED, before the arm was added: this is a DIVERGENCE and not
//! a shared reading. carve-js's `inlineText` carries
//! `case 'escaped_text': out += n.value` and carve-php's `inlineTextLeaf` has the
//! matching `instanceof EscapedText` branch, so carve-rs was the sole outlier.
//!
//! THREE arms move together, not two. `plain_inlines_parse` (the parse-time
//! index), `render::plain_inlines_typography` (the HTML renderer) and
//! `render_markdown::plain_inlines` are three spellings of one derivation, and a
//! heading id derived two ways has to be one id - so a sweep for the other
//! producers is part of the fix rather than a follow-up.
//!
//! FOUR MORE FLATTENERS carry the same gap and are NOT touched here, because they
//! derive a TERM's text rather than a heading's and would move Tier-3 ids that
//! want their own measurement: `glossary::inline_text`, `index_terms::inline_text`
//! and `color_swatch::inline_text`. Measured: `:index[a\.b]` slugs to `idx-ab-1`
//! while its display renders `a.b`, so the slug and the display already disagree
//! there.

/// The escaped run reaches the rendered heading, which is the thing the derived
/// text is supposed to be derived FROM.
#[test]
fn the_heading_renders_the_escaped_characters() {
    assert!(
        carve::to_html("# \\*bold\\* heading\n").contains("<h1>*bold* heading</h1>"),
        "{}",
        carve::to_html("# \\*bold\\* heading\n")
    );
}

/// The id MOVES where the escaped character is a word separator the slug keeps a
/// boundary for. This is the public-surface change, and it is the whole reason
/// the arm was not folded into carve-rs#798: the suite passing there meant
/// nothing pinned the ids it would move.
#[test]
fn an_escaped_separator_now_reaches_the_heading_id() {
    for (src, id) in [
        ("# a\\.b\n", "a-b"),
        ("# a\\-b\n", "a-b"),
        ("# a\\_b\n", "a-b"),
        ("# x\\!y\n", "x-y"),
    ] {
        let out = carve::to_html(src);
        assert!(
            out.contains(&format!("<section id=\"{id}\">")),
            "{src:?}: {out}"
        );
    }
}

/// CONTROL, and the reason the defect was invisible: `slugify` strips an
/// asterisk whether or not it reached the derivation, so `# \*bold\* heading`
/// and `# *bold* heading` land on the same id by two different routes. Nothing
/// here moves, and a fix keyed on "the id changed" would never have found it.
#[test]
fn control_an_escaped_asterisk_still_reaches_the_same_id() {
    for src in ["# \\*bold\\* heading\n", "# *bold* heading\n"] {
        let out = carve::to_html(src);
        assert!(
            out.contains("<section id=\"bold-heading\">"),
            "{src:?}: {out}"
        );
    }
}

/// A cross-reference resolves against the id the heading publishes, so the two
/// have to agree. They are derived by two different functions - the parse-time
/// index and the render-time id - and this is the case that fails if only one of
/// them gains the arm.
#[test]
fn a_crossref_resolves_to_the_moved_id() {
    let out = carve::to_html("# a\\.b\n\nSee </#a-b>\n");
    assert!(out.contains("<section id=\"a-b\">"), "{out}");
    assert!(out.contains("<p>See <a href=\"#a-b\">a.b</a></p>"), "{out}");
}

/// The label a resolved reference renders is the heading's cloned NODES
/// (PART 9R R4), so the escape survives there as the character the author wrote
/// - which is a second, independent reader of the same run.
#[test]
fn the_crossref_label_keeps_the_escaped_character() {
    let out = carve::to_html("# \\*bold\\* heading\n\nSee </#bold-heading>\n");
    assert!(
        out.contains("<p>See <a href=\"#bold-heading\">*bold* heading</a></p>"),
        "{out}"
    );
}

/// PART 9R R1's implicit `[label][]` index is keyed by the heading's derived
/// TEXT, so it moves with the derivation. `# a\.b` is keyed `a.b` now, and a
/// label spelling those characters resolves to it.
#[test]
fn the_by_text_index_carries_the_escaped_character() {
    let out = carve::to_html("# a\\.b\n\n[a.b][]\n");
    assert!(out.contains("<a href=\"#a-b\">a.b</a>"), "{out}");
}

/// CONTROL, and a GREEN MUTATION diagnosed rather than left: the Markdown target
/// agrees with HTML, but NOT through the arm added to
/// `render_markdown::plain_inlines`. Dropping that third arm turns nothing red.
///
/// Measured: the `{#id}` the Markdown writer emits comes from the id resolution
/// already put on the heading, so `next_heading_id`'s `explicit` branch answers
/// and the flattener is never consulted. It still feeds the writer's own dedup
/// COUNTER, whose key does not reach output in any shape that could be
/// constructed - `# a\.b` / `# a.b` / two references publishes `{#a-b}` and
/// `{#a-b-2}` identically with and without the arm.
///
/// The arm goes in anyway, because the counter's key has to agree with the
/// core's for the suffixes to line up the moment a heading reaches it without a
/// resolved id. That it is currently unpinned is the finding, and this comment
/// is where it is recorded rather than an assertion pretending to hold it.
#[test]
fn control_the_markdown_target_derives_the_same_id() {
    let doc = carve::parse("# a\\.b\n\nSee </#a-b>\n");
    let md = carve::render_markdown(&doc).expect("markdown renders");
    assert!(md.contains("(#a-b)"), "{md}");
}

/// CONTROL: an escaped character is not made into markup by reaching the
/// derivation. `\*bold\*` is four visible characters and a word, not an
/// emphasis, on every target.
#[test]
fn control_the_escape_is_still_literal_and_not_emphasis() {
    let out = carve::to_html("# \\*bold\\* heading\n");
    assert!(!out.contains("<strong>"), "{out}");
    let doc = carve::parse("# \\*bold\\* heading\n");
    let md = carve::render_markdown(&doc).expect("markdown renders");
    assert!(md.contains("\\*bold\\*"), "{md}");
}

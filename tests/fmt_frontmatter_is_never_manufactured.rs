//! `fmt` neither invents a frontmatter block nor loses one (carve-rs#732).
//!
//! Both defects live in one seam. `to_carve` reads the frontmatter block out of
//! the source itself, so that a typed or comment-bearing block survives
//! verbatim; the parsed `frontmatter` map cannot stand in for it. That makes the
//! writer a SECOND reader of the source, and a second reader is only safe while
//! it agrees with the parser about what it is reading -- the same seam
//! carve-rs#725 had to unify for the opener test.
//!
//! ## Why the corpus sweep did not catch either one
//!
//! `render_carve.rs` asserts `to_html(fmt(x)) == to_html(x)` and
//! `fmt(fmt(x)) == fmt(x)` over every corpus document, which is exactly the
//! invariant a manufactured frontmatter block violates. It never saw the shape:
//! of 690 corpus documents, exactly ONE begins with a thematic break
//! (`132-thematic-break-requires-contiguous-markers-4.crv`), and its entire
//! content is the single line `***`. A leading break with anything at all after
//! it does not appear in the corpus, so the sweep asserted the invariant for a
//! document that had nothing to lose.
//!
//! The CRLF half is worse off still: the invariant cannot see it in ANY corpus.
//! Frontmatter renders to nothing in HTML, so dropping the block leaves
//! `to_html` unchanged and only the AST moves. The two CRLF corpus documents
//! carry no frontmatter either way.
//!
//! So the assertions below are on the shapes, and the HTML invariant is joined
//! by an AST one wherever HTML is blind.

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// `to_html(fmt(x)) == to_html(x)` and `fmt(fmt(x)) == fmt(x)`, the two
/// properties the corpus sweep asserts, applied to one document.
fn assert_round_trips(src: &str) -> String {
    let formatted = fmt(src);
    assert_eq!(
        carve::to_html(&formatted),
        carve::to_html(src),
        "fmt changed what the document means:\n  ----- source -----\n{src}\n  ----- formatted --\n{formatted}"
    );
    assert_eq!(
        fmt(&formatted),
        formatted,
        "fmt is not idempotent for {src:?}"
    );
    formatted
}

#[test]
fn a_leading_thematic_break_and_a_later_one_do_not_become_frontmatter() {
    // The reported document. `***` and `---` are both thematic breaks and there
    // is no frontmatter anywhere; writing the first break as `---` made the next
    // parse read everything down to the second one as a frontmatter block.
    let src = "***\n\na\n\n---\n\nb\n";
    assert_eq!(carve::to_html(src), "<hr>\n<p>a</p>\n<hr>\n<p>b</p>");
    let formatted = assert_round_trips(src);
    assert_eq!(formatted, "***\n\na\n\n---\n\nb\n");
    assert!(carve::parse(&formatted).frontmatter_raw.is_none());
}

#[test]
fn three_lines_are_enough_to_lose_the_whole_document() {
    // The minimal form, and the one that shows the severity: the manufactured
    // block swallowed BOTH rules and the formatted document rendered nothing at
    // all. `to_html(fmt(x))` was the empty string against `<hr>\n<hr>`.
    let src = "***\n\n---\n";
    assert_eq!(carve::to_html(src), "<hr>\n<hr>");
    let formatted = assert_round_trips(src);
    assert_eq!(formatted, "***\n\n---\n");
}

#[test]
fn a_leading_break_alone_is_written_with_three_dashes() {
    // The one shape the corpus does hold (`132-...-4.crv`), and the one
    // carve#961 pins: `---` is the canonical marker and a document that is only
    // a break gets it. With nothing after it, no `---` is left to close a
    // manufactured block, so the opener test does not fire and the fallback is
    // not owed.
    //
    // This assertion previously read `"***\n"` in both directions and was
    // labelled a control. It was not one: it pinned the spelling the fallback
    // happened to produce, on the single corpus document that could have said
    // otherwise.
    assert_eq!(assert_round_trips("***\n"), "---\n");
    assert_eq!(assert_round_trips("---\n"), "---\n");
}

#[test]
fn a_leading_break_with_no_closer_after_it_keeps_three_dashes() {
    // The opener needs a CLOSER, so a leading break followed by anything that
    // is not a bare `---` line is not at risk and keeps the canonical marker.
    // This is the shape that separates "the break is on line 1" from "the text
    // would really be misread" - a guard keyed on position alone rewrites here
    // and this fails.
    assert_eq!(assert_round_trips("***\n\na\n"), "---\n\na\n");
    assert_eq!(assert_round_trips("***\n\n# T\n\nb\n"), "---\n\n# T\n\nb\n");
    // `--- ` with a trailing space is not a closer either, and the writer never
    // emits one, so a paragraph holding dashes does not arm the fallback.
    assert_eq!(assert_round_trips("***\n\n-- -\n"), "---\n\n-- -\n");
}

#[test]
fn a_leading_break_keeps_its_canonical_spelling_everywhere_else() {
    // `---` is the canonical thematic break and stays one. Only the document's
    // FIRST line can be misread, because the opener test is anchored at byte 0.
    assert_eq!(assert_round_trips("a\n\n***\n\n---\n"), "a\n\n---\n\n---\n");
    // Inside a container the line is never at byte 0 either.
    assert_eq!(assert_round_trips("> ***\n\n---\n"), "> ---\n\n---\n");
}

#[test]
fn a_closer_inside_verbatim_content_still_arms_the_fallback() {
    // The opener test is a TEXTUAL pre-pass: it scans for a bare `---` line
    // without knowing about code fences, so a `---` inside a fenced block
    // closes a manufactured frontmatter block just as a real break would. This
    // is what makes it load-bearing that the test runs on the bytes AFTER
    // `restore_verbatim`, rather than on the staged text where verbatim content
    // is still standing in for itself.
    let src = "***\n\n```\n---\n```\n";
    assert_eq!(carve::to_html(src), "<hr>\n<pre><code>---\n</code></pre>");
    assert_eq!(assert_round_trips(src), "***\n\n```\n---\n```\n");
    // And the same fence WITHOUT the `---` line does not arm it.
    assert_eq!(
        assert_round_trips("***\n\n```\na\n```\n"),
        "---\n\n```\na\n```\n"
    );
}

#[test]
fn a_leading_break_under_the_writers_own_frontmatter_keeps_three_dashes() {
    // `render_carve` writes the frontmatter MAP itself, so the break is not on
    // line 1 and no fallback is owed even though a later `---` line exists. The
    // opener test only ever sees the body, so without the `parts` emptiness
    // condition this document is rewritten on the strength of a collision that
    // cannot happen. `to_carve` cannot reach this shape - it clears the map and
    // prepends the raw block afterwards - so the tree-taking entry point is the
    // only way to assert it.
    let doc = carve::parse("---yaml\nt: 1\n---\n\n***\n\na\n\n---\n");
    assert_eq!(doc.frontmatter.len(), 1);
    let written = carve::render_carve(&doc).expect("within the render ceiling");
    assert_eq!(written, "---\nt: 1\n---\n\n---\n\na\n\n---\n");
    // And it means what it said: one frontmatter block, two rules.
    assert_eq!(carve::to_html(&written), "<hr>\n<p>a</p>\n<hr>");
}

#[test]
fn a_real_leading_frontmatter_block_still_opens_with_three_dashes() {
    // CONTROL, and the boundary of the rewrite: the guard must not reach a
    // document whose first line is genuine frontmatter.
    let src = "---yaml\nt: 1\n---\n\n***\n\n---\n";
    let formatted = assert_round_trips(src);
    assert!(formatted.starts_with("---yaml\n"));
    let raw = carve::parse(&formatted)
        .frontmatter_raw
        .expect("frontmatter");
    assert_eq!(raw.format, "yaml");
    assert_eq!(raw.content, "t: 1");
}

#[test]
fn the_tree_taking_writer_keeps_three_dashes_under_its_own_frontmatter() {
    // `render_carve` is public and writes the frontmatter MAP itself, so on that
    // path the break is not the first line and `---` is still right. `to_carve`
    // cannot reach this: it clears the map and prepends the raw block
    // afterwards, so the block list it renders never has frontmatter in front of
    // it. Rewriting unconditionally therefore passes every other test in this
    // file, and fails here -- which is what makes the condition load-bearing
    // rather than decorative.
    let doc = carve::parse("---yaml\nt: 1\n---\n\n***\n\na\n");
    assert_eq!(doc.frontmatter.len(), 1);
    assert_eq!(
        carve::render_carve(&doc).expect("within the render ceiling"),
        "---\nt: 1\n---\n\n---\n\na\n"
    );
}

#[test]
fn an_attributed_leading_break_is_not_rewritten() {
    // CONTROL. A break carrying block attributes writes its `{...}` line first,
    // so `---` is not on line 1 and cannot be misread. The guard tests the
    // rendered bytes rather than the node kind, so this is what keeps it from
    // rewriting a break it did not need to.
    let formatted = assert_round_trips("{.x}\n***\n\n---\n");
    assert_eq!(formatted, "{.x}\n---\n\n---\n");
}

// ---------------------------------------------------------------------------
// The CRLF half. Every source here builds its line endings from `\r\n` escapes
// in the Rust literal, never from a fixture FILE: a checked-in file passes
// through git's line-ending handling and any editor that touches it, and a
// CRLF fixture that silently becomes LF is a duplicate of the case above it
// rather than a test. `the_crlf_sources_really_carry_carriage_returns` asserts
// that they still do.
// ---------------------------------------------------------------------------

#[test]
fn the_crlf_sources_really_carry_carriage_returns() {
    // The guard that keeps the four tests below from quietly becoming copies of
    // their LF controls. If normalization ever reaches these literals, this
    // fails first and names the reason.
    for src in [
        "---toml\r\na = 1\r\n---\r\n\r\nbody\r\n",
        "---yaml\r\nt: 1\r\n---\r\n\r\nbody\r\n",
    ] {
        assert!(src.contains('\r'), "{src:?} lost its carriage returns");
        assert!(src.contains("\r\n"), "{src:?} is not CRLF");
    }
}

#[test]
fn crlf_keeps_a_format_the_map_cannot_represent() {
    // The severe half. `a = 1` is not `key: value`, so the parsed map is EMPTY
    // and `render_frontmatter` had nothing to write: the whole block vanished
    // and `fmt` returned just `body`. HTML cannot see this -- frontmatter
    // renders to nothing - so the AST is what asserts it.
    let src = "---toml\r\na = 1\r\n---\r\n\r\nbody\r\n";
    let formatted = assert_round_trips(src);
    assert_eq!(formatted, "---toml\na = 1\n---\n\nbody\n");

    let before = carve::parse(src).frontmatter_raw.expect("frontmatter");
    let after = carve::parse(&formatted)
        .frontmatter_raw
        .expect("frontmatter survived fmt");
    assert_eq!(after.format, before.format);
    assert_eq!(after.content, before.content);
    assert_eq!(after.format, "toml");
}

#[test]
fn crlf_keeps_the_format_token() {
    // The milder half, and the one the ticket reported: `title: t` DOES parse
    // into the map, so the block was rebuilt - but `render_frontmatter` writes a
    // bare `---`, and the format token was lost. A consumer routing on the token
    // silently gets yaml.
    let src = "---yaml\r\nt: 1\r\n---\r\n\r\nbody\r\n";
    let formatted = assert_round_trips(src);
    assert_eq!(formatted, "---yaml\nt: 1\n---\n\nbody\n");
    assert_eq!(
        carve::parse(&formatted)
            .frontmatter_raw
            .expect("frontmatter")
            .format,
        "yaml"
    );
}

#[test]
fn a_lone_carriage_return_reaches_the_same_seam() {
    // Old-Mac line endings normalize to LF for the parser too, so the writer
    // has to read them the same way. Same fall-through, same loss.
    let src = "---toml\ra = 1\r---\r\rbody\r";
    assert!(src.contains('\r') && !src.contains("\r\n"));
    let formatted = assert_round_trips(src);
    assert_eq!(formatted, "---toml\na = 1\n---\n\nbody\n");
}

#[test]
fn a_byte_order_mark_reaches_the_same_seam() {
    // The BOM is stripped before the parser looks for `---`, so a BOM'd document
    // HAS frontmatter. The writer's `starts_with("---")` ran on the raw string
    // and said it did not.
    let src = "\u{feff}---toml\na = 1\n---\n\nbody\n";
    let formatted = assert_round_trips(src);
    assert_eq!(formatted, "---toml\na = 1\n---\n\nbody\n");
}

#[test]
fn lf_input_was_already_right() {
    // CONTROL. The same documents with LF endings and no BOM: these went through
    // the raw-source path and always kept their token, which is why the defect
    // needed a line-ending or a BOM to show up at all.
    assert_eq!(
        assert_round_trips("---toml\na = 1\n---\n\nbody\n"),
        "---toml\na = 1\n---\n\nbody\n"
    );
    assert_eq!(
        assert_round_trips("---yaml\nt: 1\n---\n\nbody\n"),
        "---yaml\nt: 1\n---\n\nbody\n"
    );
}

#[test]
fn a_crlf_document_with_no_frontmatter_is_unaffected() {
    // CONTROL. Normalizing the writer's view of the source must not change a
    // document that has no block to find.
    assert_eq!(assert_round_trips("# T\r\n\r\na\r\nb\r\n"), "# T\n\na\nb\n");
}

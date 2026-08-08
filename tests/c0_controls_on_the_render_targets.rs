//! C0 CONTROLS ON THE RENDER TARGETS (spec PART 9 §29, markup-carve/carve#979;
//! carve-rs#812).
//!
//! §29 is the companion to PART 7's ONE WHITESPACE DEFINITION, IN EVERY
//! CONSTRUCT. That clause governs what the LANGUAGE reads; this one governs what
//! a TARGET may emit, and the two were being answered as one. After
//! markup-carve/carve#963 the whitespace of the language is exactly U+0020,
//! U+0009, U+000A and U+000D, and EVERY other C0 control - U+0000..U+0008,
//! U+000B, U+000C, U+000E..U+001F - is ordinary content.
//!
//! | target | non-whitespace C0 control |
//! | --- | --- |
//! | HTML | emitted (T1) |
//! | Markdown | emitted (T2) |
//! | plain | emitted (T3) |
//! | ANSI | stripped (T4) |
//!
//! The subject is the CLASS, not the two characters the whitespace clause
//! happens to name, so every test below runs the whole class rather than the
//! vertical tab and the form feed alone.

/// U+0000..U+0008, U+000B, U+000C, U+000E..U+001F: every C0 control that is not
/// whitespace.
fn non_whitespace_c0() -> Vec<char> {
    (0x00u32..0x09)
        .chain([0x0B, 0x0C])
        .chain(0x0E..0x20)
        .map(|c| char::from_u32(c).expect("a C0 code point is a char"))
        .collect()
}

/// NUL never reaches a target: the parser replaces it with U+FFFD, so a test
/// that asserted on the byte would be asserting about the PARSER rather than
/// about §29. The replacement is asserted on its own below.
fn emitted_class() -> Vec<char> {
    non_whitespace_c0()
        .into_iter()
        .filter(|c| *c != '\0')
        .collect()
}

fn render(target: &str, src: &str) -> String {
    let doc = carve::parse(src);
    match target {
        "html" => carve::to_html(src),
        "markdown" => carve::render_markdown(&doc).expect("markdown renders"),
        "plain" => carve::render_plain_text(&doc).expect("plain renders"),
        "ansi" => carve::render_ansi(&doc).expect("ansi renders"),
        other => panic!("unknown target {other}"),
    }
}

// ---------------------------------------------------------------------------
// T1, T2, T3: the class is emitted.
// ---------------------------------------------------------------------------

/// The probe §29's ticket names, on each of the three fidelity targets.
#[test]
fn the_vertical_tab_and_form_feed_probe_survives_html_markdown_and_plain() {
    for target in ["html", "markdown", "plain"] {
        let out = render(target, "a\u{0b}b\u{0c}c\n");
        assert!(
            out.contains("a\u{0b}b\u{0c}c"),
            "{target} did not emit the probe: {out:?}"
        );
    }
}

/// The whole class, one character at a time, because §29 is stated about the
/// class and a test of two characters would leave the rest to habit - which is
/// the state the clause replaces.
#[test]
fn every_non_whitespace_c0_control_reaches_html_markdown_and_plain() {
    for target in ["html", "markdown", "plain"] {
        let missing: Vec<String> = emitted_class()
            .into_iter()
            .filter(|c| !render(target, &format!("a{c}b\n")).contains(*c))
            .map(|c| format!("U+{:04X}", c as u32))
            .collect();
        assert!(
            missing.is_empty(),
            "{target} stripped {}",
            missing.join(", ")
        );
    }
}

/// A control inside a CODE SPAN and a code block is content too. The Markdown
/// target sends those through the same choke point as prose, so a strip that
/// came back only there would pass the prose case above.
#[test]
fn a_control_inside_code_reaches_markdown_and_plain() {
    for target in ["markdown", "plain"] {
        let out = render(target, "`a\u{0b}b`\n");
        assert!(out.contains("a\u{0b}b"), "{target} inline code: {out:?}");
        let out = render(target, "```\na\u{0c}b\n```\n");
        assert!(out.contains("a\u{0c}b"), "{target} code block: {out:?}");
    }
}

// ---------------------------------------------------------------------------
// T4: ANSI strips them, and keeps its strip BROAD.
// ---------------------------------------------------------------------------

/// The terminal is the one consumer that ACTS on the character: a form feed
/// feeds or clears, and U+001B introduces a sequence that can move the cursor,
/// rewrite earlier output or reach the clipboard.
#[test]
fn ansi_strips_every_non_whitespace_c0_control() {
    let leaked: Vec<String> = non_whitespace_c0()
        .into_iter()
        .filter(|c| render("ansi", &format!("a{c}b\n")).contains(*c))
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    assert!(leaked.is_empty(), "ansi emitted {}", leaked.join(", "));
}

/// T4 DOES NOT NARROW THE TERMINAL TARGET. §25 NON-HTML TARGETS requires DEL and
/// the C1 controls to go too, because CSI (U+009B) and OSC (U+009D) are
/// single-character forms of the sequences that requirement exists to stop.
/// Narrowing the ANSI strip to C0 is a security regression, and §29 T4 says so.
#[test]
fn ansi_still_strips_del_and_the_c1_controls() {
    let class: Vec<char> = std::iter::once('\u{7f}')
        .chain((0x80u32..0xA0).map(|c| char::from_u32(c).expect("C1 is a char")))
        .collect();
    let leaked: Vec<String> = class
        .into_iter()
        .filter(|c| render("ansi", &format!("a{c}b\n")).contains(*c))
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    assert!(leaked.is_empty(), "ansi emitted {}", leaked.join(", "));
}

/// The terminal keeps stripping inside code and a destination as well, which is
/// where an escape sequence would be least visible to a reader of the source.
///
/// The assertion looks for the AUTHOR's sequence rather than for the ESC byte:
/// the ANSI renderer emits its own styling escapes, which is the whole point of
/// the target, so `!contains('\u{1b}')` would be measuring the renderer instead
/// of the author. What must not survive is the author's `\u{1b}[`.
#[test]
fn ansi_strips_a_control_inside_code_and_a_destination() {
    let out = render("ansi", "`a\u{1b}[31mb`\n");
    assert!(out.contains("a[31mb"), "the author's ESC survived: {out:?}");
    let out = render("ansi", "[t](/u\u{1b}x)\n");
    assert!(!out.contains("u\u{1b}x"), "{out:?}");
}

// ---------------------------------------------------------------------------
// CONTROLS: what §29 does not move.
// ---------------------------------------------------------------------------

/// CONTROL, and the one row that is about the PARSER rather than about a target:
/// U+0000 is replaced with U+FFFD on the way in, so no target ever sees it. The
/// spec does not rule on this and the behavior is unchanged here; the assertion
/// exists so a later change to §29's class is not read as licensing a raw NUL in
/// output.
#[test]
fn control_a_nul_is_already_a_replacement_character_before_any_target() {
    for target in ["html", "markdown", "plain", "ansi"] {
        let out = render(target, "a\0b\n");
        assert!(!out.contains('\0'), "{target} emitted a raw NUL: {out:?}");
        if target != "ansi" {
            assert!(out.contains('\u{fffd}'), "{target}: {out:?}");
        }
    }
}

/// CONTROL: DEL and the C1 controls on the Markdown and plain targets are
/// OUTSIDE §29 (T5 says so, and leaves them to a ticket of their own). They were
/// stripped before this change and are stripped after it, so this change neither
/// fixes nor introduces that defect.
#[test]
fn control_markdown_and_plain_still_strip_del_and_the_c1_controls() {
    for target in ["markdown", "plain"] {
        for c in std::iter::once('\u{7f}')
            .chain((0x80u32..0xA0).map(|c| char::from_u32(c).expect("C1 is a char")))
        {
            let out = render(target, &format!("a{c}b\n"));
            assert!(
                !out.contains(c),
                "{target} emitted U+{:04X}: {out:?}",
                c as u32
            );
        }
    }
}

/// CONTROL: §26 is a different rule from §29 and is untouched here. A bidi
/// override is an injection vector in a renderer, which a C0 control is not, and
/// the HTML target removes it.
///
/// MEASURED, NOT ASSUMED: this engine strips a bidi control on the HTML target
/// ONLY - the Markdown, plain and ANSI targets emit it, on `main` at `04f9284`
/// exactly as here. Whether §26's reach should be wider is a separate question
/// this change does not touch and must not be read as answering; the case is
/// here so that the C0 narrowing cannot be blamed for it either way.
#[test]
fn control_a_bidi_override_is_removed_on_html_and_unchanged_elsewhere() {
    assert!(!render("html", "a\u{202e}b\n").contains('\u{202e}'));
    for target in ["markdown", "plain", "ansi"] {
        let out = render(target, "a\u{202e}b\n");
        assert!(out.contains('\u{202e}'), "{target}: {out:?}");
    }
}

/// CONTROL: U+000D is WHITESPACE after markup-carve/carve#963, so §29's class -
/// "every OTHER C0 control" - excludes it and this change rules on it not at all.
/// The parser normalizes a CRLF before any block is read, so one can only arrive
/// on a tree built through the API; there it is a line terminator inside a leaf
/// the writer is laying out in lines of its own, and both targets drop it
/// exactly as they did before (raised by `codex review`).
#[test]
fn control_a_carriage_return_in_a_constructed_tree_is_still_dropped() {
    use std::collections::BTreeMap;
    let doc = carve::Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        source_len: 0,
        ingest_payload_len: 0,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            at_content_column: false,
            pos: None,
            attrs: None,
            children: vec![carve::InlineNode::text("a\rb".to_string())],
        })],
    };
    for (target, out) in [
        ("markdown", carve::render_markdown(&doc)),
        ("plain", carve::render_plain_text(&doc)),
        ("ansi", carve::render_ansi(&doc)),
    ] {
        let out = out.expect("renders");
        assert!(!out.contains('\r'), "{target}: {out:?}");
        assert!(out.contains("ab"), "{target}: {out:?}");
    }
}

/// CONTROL: the four whitespace characters are unchanged. A tab and a newline
/// pass through every target as they always did; this change is about the
/// characters that are NOT whitespace.
#[test]
fn control_a_tab_still_reaches_the_markdown_and_plain_targets() {
    for target in ["markdown", "plain"] {
        let out = render(target, "```\na\tb\n```\n");
        assert!(out.contains("a\tb"), "{target}: {out:?}");
    }
}

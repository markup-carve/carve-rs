//! The parser rebuilds its source several times and every consumer splits the
//! result again with `str::lines()`. `join("\n")` alone makes that round trip
//! lossy in exactly one place - a trailing EMPTY line:
//!
//! ```text
//! ["a", ""]  ->  join  ->  "a\n"    ->  lines()  ->  ["a"]        lost
//! ["a", ""]  ->  +\n   ->  "a\n\n"   ->  lines()  ->  ["a", ""]    preserved
//! ["a"]      ->  +\n   ->  "a\n"    ->  lines()  ->  ["a"]        unchanged
//! ```
//!
//! Every other line survives a plain `join` because the separator before it is
//! still there; only the last has nothing after it to imply it. `lines()` drops
//! one trailing newline, so terminating changes the empty-last-line case alone.
//!
//! This covers the DOCUMENT-level rebuilds (the body after definitions are
//! lifted out, and `extract_link_defs`). The container-level rebuilds are NOT
//! terminated - see the module note at the bottom (markup-carve/carve-rs#908).

fn code(source: &str) -> String {
    let html = carve::to_html(source);
    let start = html
        .find("<code")
        .and_then(|i| html[i..].find('>').map(|j| i + j + 1));
    match start {
        Some(s) => match html[s..].find("</code>") {
            Some(e) => html[s..s + e].to_string(),
            None => "(unterminated)".to_string(),
        },
        None => "(no code block)".to_string(),
    }
}

#[test]
fn an_eof_closed_fence_keeps_its_trailing_blank() {
    assert_eq!(code("```\nx\n\n"), "x\n\n");
    assert_eq!(code("```\nx\n\n\n"), "x\n\n\n");
    assert_eq!(code("~~~\nx\n\n"), "x\n\n");
}

#[test]
fn a_link_definition_ahead_of_the_fence_does_not_cost_a_blank() {
    // This one isolates `extract_link_defs`: the same document without the
    // definition already worked, so the definition added the rebuild that lost
    // the line.
    assert_eq!(code("[r]: /u\n\n```\nx\n\n"), "x\n\n");
}

#[test]
fn a_body_that_is_only_blank_lines_keeps_them() {
    assert_eq!(code("```\n\n\n"), "\n\n");
}

/// BOUNDS. None of these move when the terminator is removed - they pin what
/// the change must not touch, and are not evidence for it.
#[test]
fn control_the_shapes_that_never_had_a_trailing_blank() {
    assert_eq!(code("```\nx\n```\n"), "x\n");
    assert_eq!(code("```\nx\n"), "x\n");
    assert_eq!(code("```\nx"), "x\n");
    assert_eq!(code("```\nx\n\ny\n"), "x\n\ny\n");
}

/// A fence ended by a CONTAINER's closer keeps its blank now: `LineBuffer`
/// records whether its last line was authored or synthetic, so `into_source`
/// terminates for the author's blank and not for a `push_synthetic_blank`.
#[test]
fn a_container_closed_fence_keeps_its_trailing_blank() {
    assert_eq!(code("::: note\n```\nx\n\n:::\n"), "x\n\n");
    assert_eq!(code("::: note\n```\nx\n\n\n:::\n"), "x\n\n\n");
    assert_eq!(code("> ```\n> x\n>\n"), "x\n\n");
}

/// KNOWN STILL WRONG, asserted so the remaining gap is visible rather than
/// implied. A fence ending with the LIST ITEM loses its blank for a different
/// reason: the collection loop stops without consuming it, so `lines` is
/// already `["x"]` before any join runs and there is nothing to preserve. That
/// is the collector half, the one carve-js#989 needed separately.
/// When it lands, this assertion flips and this test is deleted.
#[test]
fn known_gap_an_item_final_fence_still_loses_it() {
    assert_eq!(code("- ```\n  x\n\n"), "x\n");
}

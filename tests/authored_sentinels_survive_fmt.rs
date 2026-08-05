//! An authored private-use character survives `fmt` (PART 11 §1, carve-rs#607).
//!
//! The writer stages two markers whose restore is POSITIONAL: `VERBATIM_BLANK`
//! for a line that was blank inside verbatim content, and the thematic guard
//! for a line that would otherwise re-parse as a break. A character the AUTHOR
//! wrote in that same position was indistinguishable from one the writer
//! inserted, and restore ate it - so `to_html(fmt(x)) != to_html(x)`.
//!
//! carve-rs#613 narrowed the restore to those exact positions, which fixed
//! every INLINE placement. It could not fix the line-alone one: that ambiguity
//! is positional, and narrowing has nothing left to narrow. The character moves
//! instead, chosen per render from one the document does not contain.
//!
//! The failing shape was NOT limited to a code block. Restore runs over the
//! whole joined document, so a paragraph and a block quote lost the character
//! the same way.

use carve::{to_carve, to_html};

const VERBATIM_BLANK: char = '\u{e003}';
const THEMATIC_GUARD: char = '\u{e004}';
const ESCAPED_SPACE: char = '\u{e010}';
const STAGED_SPACE: char = '\u{e011}';
const STAGED_TAB: char = '\u{e012}';

/// PART 11 §1: formatting may change the spelling, never the document.
fn round_trips(src: &str) -> bool {
    to_html(&to_carve(src)) == to_html(src)
}

#[test]
fn an_authored_blank_marker_alone_on_a_line_survives() {
    for (label, src) in [
        ("paragraph", format!("a\n{VERBATIM_BLANK}\nb\n")),
        ("code block", format!("```\na\n{VERBATIM_BLANK}\nb\n```\n")),
        ("tilde fence", format!("~~~\na\n{VERBATIM_BLANK}\nb\n~~~\n")),
        ("block quote", format!("> q\n> {VERBATIM_BLANK}\n> r\n")),
    ] {
        assert!(round_trips(&src), "fmt changed the document in a {label}");
        assert!(
            to_html(&to_carve(&src)).contains(VERBATIM_BLANK),
            "the character was deleted in a {label}"
        );
    }
}

#[test]
fn an_authored_thematic_guard_alone_on_a_line_survives() {
    for (label, src) in [
        ("paragraph", format!("a\n{THEMATIC_GUARD}\nb\n")),
        ("code block", format!("```\na\n{THEMATIC_GUARD}\nb\n```\n")),
    ] {
        assert!(round_trips(&src), "fmt changed the document in a {label}");
    }
}

#[test]
fn an_authored_marker_beside_a_real_one_survives() {
    // The case the counting has to get exactly right: the document holds one
    // authored marker AND the writer inserts one for a genuine blank line, so
    // the counts differ by one rather than by everything.
    let src = format!("```\na\n{VERBATIM_BLANK}\n\nb\n```\n");
    assert!(round_trips(&src), "fmt changed the document");
    assert!(
        to_html(&to_carve(&src)).contains(VERBATIM_BLANK),
        "the authored character was deleted"
    );
}

#[test]
fn no_sentinel_leaks_into_the_output() {
    // A replacement character that escaped into the document would be invisible
    // in review and corrupt the file. The only private-use character in the
    // output must be the one the author wrote.
    let src = format!("```\na\n{VERBATIM_BLANK}\nb\n```\n");
    let out = to_carve(&src);
    let pua: Vec<char> = out
        .chars()
        .filter(|c| ('\u{e000}'..='\u{f8ff}').contains(c))
        .collect();
    assert_eq!(
        pua,
        vec![VERBATIM_BLANK],
        "a staging sentinel reached the output"
    );
}

#[test]
fn the_markers_still_do_their_job() {
    // The controls. Both markers exist for a reason and the retry must not
    // disable them.
    //
    // A blank line inside verbatim content is content and survives.
    let code = "```\na\n\nb\n```\n";
    assert!(round_trips(code));
    assert!(
        to_html(code).contains("a\n\nb"),
        "the blank line inside the code block was lost: {}",
        to_html(code)
    );

    // A paragraph line of three dashes must not come back as a thematic break.
    for src in ["a\n\n---\n\nb\n", "para\n\n----\n"] {
        assert!(round_trips(src), "fmt changed {src:?}");
    }
}

#[test]
fn fmt_is_still_idempotent() {
    for src in [
        format!("```\na\n{VERBATIM_BLANK}\nb\n```\n"),
        format!("a\n{THEMATIC_GUARD}\nb\n"),
        "```\na\n\nb\n```\n".to_string(),
    ] {
        let once = to_carve(&src);
        assert_eq!(to_carve(&once), once, "second pass differs for {src:?}");
    }
}

#[test]
fn a_document_with_no_private_use_character_is_unaffected() {
    // The common case takes the first render: the counts match and nothing is
    // repeated. Pinned as behaviour rather than as a timing claim - the output
    // must be exactly what it was before the retry existed.
    assert_eq!(to_carve("# H\n\ntext\n"), "# H\n\ntext\n");
    assert_eq!(to_carve("```\na\n\nb\n```\n"), "```\na\n\nb\n```\n");
}

#[test]
fn the_globally_restored_markers_survive_too() {
    // carve-rs#630. These three are undone by a GLOBAL replace rather than by
    // position - each has more than one insertion site - so an authored one was
    // eaten in EVERY placement, not only alone on a line. The same counting
    // covers them.
    for marker in [ESCAPED_SPACE, STAGED_SPACE, STAGED_TAB] {
        for (label, src) in [
            ("code block, alone", format!("```\na\n{marker}\nb\n```\n")),
            ("code block, inline", format!("```\na{marker}b\n```\n")),
            ("paragraph, alone", format!("a\n{marker}\nb\n")),
            ("paragraph, inline", format!("x{marker}y\n")),
        ] {
            assert!(
                round_trips(&src),
                "fmt changed the document for U+{:04X} in a {label}",
                marker as u32
            );
        }
    }
}

#[test]
fn no_private_use_code_point_is_corrupted() {
    // The property worth being able to state, rather than one codepoint at a
    // time. U+E000..U+E0FF covers every marker this writer stages and the
    // published no-break-space placeholder, with room either side.
    let mut broken = Vec::new();
    for cp in 0xe000u32..=0xe0ff {
        let c = char::from_u32(cp).unwrap();
        for src in [format!("```\na\n{c}\nb\n```\n"), format!("x{c}y\n")] {
            if !round_trips(&src) {
                broken.push(format!("U+{cp:04X}"));
                break;
            }
        }
    }
    assert!(broken.is_empty(), "corrupted: {}", broken.join(", "));
}

#[test]
fn the_staged_markers_still_do_their_job() {
    // The controls for the three added above. Each exists to carry something
    // through escaping, and the retry must not disable any of them.
    //
    // An escaped space stays an escape rather than becoming a literal nbsp.
    assert_eq!(to_carve("10\\ kg\n"), "10\\ kg\n");
    // Trailing whitespace inside verbatim content is preserved.
    let code = "```\na   \nb\n```\n";
    assert!(round_trips(code));
    assert!(
        to_html(code).contains("a   "),
        "trailing run lost: {}",
        to_html(code)
    );
    // A line block's leading indentation survives.
    let lb = "::: |\n  Violets are blue.\n:::\n";
    assert!(round_trips(lb));
}

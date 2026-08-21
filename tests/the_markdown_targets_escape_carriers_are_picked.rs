//! THE MARKDOWN TARGET'S ESCAPE CARRIERS ARE PICKED PER DOCUMENT, NOT FIXED
//! (markup-carve/carve-rs#1216, the port of markup-carve/carve-js#1289).
//!
//! PART 11 §8a and §8b decide three escapes - `_`, `#` and `[` - on the EMITTED
//! LINE rather than on the node, so the writer carries the undecided candidates
//! to the line inside private-use markers and resolves them there. The markers
//! were the fixed run U+E004..U+E007, and the strip on the way in then DELETED
//! anything left in that range. An author who wrote one of those four code
//! points lost it outright, and only on this target - which is what says it was
//! the writer's marker rather than a decision about the character.
//!
//! PART 9 §29 already settled the same question for the C0 controls: every
//! character that is not one of the four whitespace characters is CONTENT
//! (PART 7), and a target that silently deletes content is lossy rather than
//! safe. A fenced code block is the row that decides it on its own - the block
//! is verbatim content, so a character disappearing out of one is a content
//! change the format is supposed to make impossible.
//!
//! The remedy is markup-carve/carve#678's, the one the canonical writer in this
//! crate already runs and the parser has run since markup-carve/carve-rs#1218:
//! pick the carriers from code points the document does not contain. Then no
//! authored character can be one, and the deletion the strip existed for has
//! nothing left to do.
//!
//! Every character here is written as a code point rather than as a literal: a
//! private-use character is invisible in a rendered string, which is exactly how
//! the defect hid.

/// The four the Markdown target reserved.
const RESERVED: [u32; 4] = [0xE004, 0xE005, 0xE006, 0xE007];

/// Private-use code points NO mechanism in this crate claims, so a difference
/// against one of these can only be the reservation.
const CONTROLS: [u32; 4] = [0xE001, 0xE002, 0xE008, 0xE015];

/// One authored character in each context the ticket measured.
const CONTEXTS: [(&str, &str); 7] = [
    ("paragraph", "a@b\n"),
    ("code span", "`a@b`\n"),
    ("code block", "```\na@b\n```\n"),
    ("heading", "# a@b\n"),
    ("link text", "[a@b](/u)\n"),
    ("table cell", "| a@b | y |\n| --- | --- |\n| 1 | 2 |\n"),
    ("raw html", "```=html\n<p>a@b</p>\n```\n"),
];

fn at(code: u32) -> char {
    char::from_u32(code).expect("a code point")
}

fn shape(template: &str, code: u32) -> String {
    template.replace('@', &at(code).to_string())
}

/// The rendered document with the payload character normalized away, so two
/// readings of the same shape differ only where the STRUCTURE differs.
fn canonical(rendered: &str, code: u32) -> String {
    rendered.replace(at(code), "\u{2603}")
}

/// THE TABLE THE TICKET MEASURED: every reserved code point, every context.
///
/// Read against the same document holding a code point nothing claims, so only
/// the reservation can explain a difference. Before the fix all four differed in
/// all seven contexts - 28 of 28 - by being deleted.
#[test]
fn a_reserved_code_point_survives_every_context() {
    for (name, template) in CONTEXTS {
        for reserved in RESERVED {
            for control in CONTROLS {
                assert_eq!(
                    canonical(&carve::to_markdown(&shape(template, control)), control),
                    canonical(&carve::to_markdown(&shape(template, reserved)), reserved),
                    "U+{reserved:04X} does not read as content in a {name} \
                     (against U+{control:04X})"
                );
            }
            assert!(
                carve::to_markdown(&shape(template, reserved)).contains(at(reserved)),
                "U+{reserved:04X} did not reach the output from a {name}"
            );
        }
    }
}

/// A CODE BLOCK IS VERBATIM CONTENT, stated on its own rather than as one row of
/// the table above: it is the row that makes this a content bug rather than a
/// rendering preference. The block must come back byte for byte.
#[test]
fn a_code_block_returns_what_the_author_wrote() {
    for reserved in RESERVED {
        let source = format!("```\nlet x = 1;{}\n```\n", at(reserved));
        assert_eq!(carve::to_markdown(&source), source);
    }
}

/// A whole run at once, which is the case a picker that moved only the single
/// occupied slot would fail: the four are allocated as a RUN, so a document
/// holding all four has to push the run past all of them.
#[test]
fn a_document_holding_the_whole_reserved_run_keeps_it() {
    let run: String = RESERVED.iter().map(|code| at(*code)).collect();
    let out = carve::to_markdown(&format!("a{run}b\n"));

    assert_eq!(out, format!("a{run}b\n"));
}

/// The pool is 6399 code points wide and the scan walks it ONE CODE POINT AT A
/// TIME, so a document occupying a long prefix of it still finds a run. What
/// must not happen is a refusal, an empty render or a hang.
#[test]
fn a_document_occupying_a_long_prefix_of_the_pool_still_renders() {
    let occupied: String = (0xE001u32..0xE001 + 2000).map(at).collect();
    let out = carve::to_markdown(&format!("{occupied}\n"));

    assert!(out.contains(at(0xE001)));
    assert!(out.contains(at(0xE001 + 1999)));
}

/// Exhaustion FALLS BACK rather than refusing, which is the writer-side answer
/// markup-carve/carve-js#1289 settled on and markup-carve/carve-rs#1218 already
/// ports for the parser. A document occupying the whole pool cannot be given a
/// free run, and a writer that gives up on it is worse than one that behaves as
/// it did before the run was picked at all.
#[test]
fn a_document_occupying_the_whole_pool_still_renders() {
    let whole: String = (0xE001u32..=0xF8FF).map(at).collect();
    let out = carve::to_markdown(&format!("a{whole}b\n"));

    assert!(out.starts_with('a'), "{:?}", &out[..8.min(out.len())]);
    assert!(out.contains(at(0xF8FF)));
}

/// THE CARRIERS' REASON, which a fix that deleted the mechanism would re-break.
///
/// §8a decides on the emitted line, so an underscore inside an identifier keeps
/// no escape and a doubled one adjacent to a live delimiter does. §8b M2b asks
/// where on the line a hash stands, and the answer has to survive the container
/// prefix in front of the line.
#[test]
fn the_deferred_escape_decisions_still_run_on_the_emitted_line() {
    let rows: [(&str, &str); 6] = [
        ("company_id\n", "company_id\n"),
        ("a __b\n", "a \\_\\_b\n"),
        ("a \\# b\n", "a # b\n"),
        ("\\# literal\n", "\\# literal\n"),
        ("> \\# quoted\n", "> \\# quoted\n"),
        ("a \\[b\\] c\n", "a \\[b\\] c\n"),
    ];

    for (source, expected) in rows {
        assert_eq!(carve::to_markdown(source), expected, "{source:?}");
    }
}

/// BOTH ROLES AT ONCE: the document occupies a carrier AND needs every decision
/// above. The picked run has to move without moving an answer.
#[test]
fn the_decisions_are_unchanged_with_an_authored_carrier_on_the_line() {
    for reserved in RESERVED {
        let carrier = at(reserved);
        assert_eq!(
            carve::to_markdown(&format!("{carrier}company_id\n")),
            format!("{carrier}company_id\n")
        );
        assert_eq!(
            carve::to_markdown(&format!("{carrier} a __b\n")),
            format!("{carrier} a \\_\\_b\n")
        );
        // The carrier goes AFTER the hash here. A private-use character is
        // content, so one in FRONT of the hash takes it off the line's content
        // position and M2b drops the escape - correctly, and for a reason that
        // has nothing to do with which code point carries the decision.
        assert_eq!(
            carve::to_markdown(&format!("\\# literal {carrier}\n")),
            format!("\\# literal {carrier}\n")
        );
    }
}

/// CONTROL: THE NARROWING DOES NOT REACH THE CONTROLS §29 EXCLUDES.
///
/// DEL and the C1 block sit outside PART 9 §29 by T5, and CSI (U+009B) and OSC
/// (U+009D) are single-character forms of the sequences §25 exists to stop. A
/// fix that widened "stop deleting the private-use range" into "stop deleting
/// anything" would pass every case in this file except this one.
#[test]
fn control_del_and_the_c1_controls_are_still_dropped() {
    for code in [0x7Fu32, 0x80, 0x9B, 0x9D, 0x9F] {
        assert_eq!(
            carve::to_markdown(&format!("a{}b\n", at(code))),
            "ab\n",
            "U+{code:04X} reached the output"
        );
    }
}

/// CONTROL: the other targets never deleted these, and still do not. The defect
/// was this target's alone, which is what identified it as a writer's marker
/// rather than a reading of the character.
#[test]
fn control_the_other_targets_are_untouched() {
    for reserved in RESERVED {
        let source = format!("a{}b\n", at(reserved));
        assert!(carve::to_html(&source).contains(at(reserved)));
        assert!(carve::to_plain_text(&source).contains(at(reserved)));
        assert!(carve::to_carve(&source).contains(at(reserved)));
    }
}

/// CONTROL: a document with no private-use character - every real one - is
/// unaffected. The pick keeps the preferred run for it, so nothing about the
/// output moves.
#[test]
fn control_an_ordinary_document_is_unchanged() {
    assert_eq!(
        carve::to_markdown("# Title\n\nA *bold* word, `code_span`, and a [link](/u).\n"),
        "# Title\n\nA **bold** word, `code_span`, and a [link](/u).\n"
    );
}

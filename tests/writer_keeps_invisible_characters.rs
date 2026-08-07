//! `fmt` WRITES BACK WHAT THE TREE HOLDS, invisible or not.
//!
//! PART 11's invariant is `to_html(fmt(x)) == to_html(x)`. Three producers in
//! the writer lost content the parser and every renderer keep, so any document
//! holding one of these characters came back different - and no corpus document
//! held one until carve#890 and carve#926 added theirs.
//!
//! All three are the same shape as the parser defects those rulings corrected:
//! a rule about two characters, written as a language's whitespace class.

use carve::{to_carve, to_html};

fn round_trips(src: &str) -> bool {
    to_html(&to_carve(src)) == to_html(src)
}

#[test]
fn a_line_holding_one_exotic_space_is_not_written_back_empty() {
    // `blank_line` holds spaces and tabs and NOTHING ELSE (carve#890), so each
    // of these lines is CONTENT. Written back empty, it re-read as a blank and
    // split its paragraph in two.
    for (name, ch) in [
        ("OGHAM SPACE MARK", '\u{1680}'),
        ("EN QUAD", '\u{2000}'),
        ("THIN SPACE", '\u{2009}'),
        ("HAIR SPACE", '\u{200a}'),
        ("NARROW NO-BREAK SPACE", '\u{202f}'),
        ("MEDIUM MATHEMATICAL SPACE", '\u{205f}'),
        ("IDEOGRAPHIC SPACE", '\u{3000}'),
        ("NO-BREAK SPACE", '\u{a0}'),
        ("ZERO WIDTH SPACE", '\u{200b}'),
    ] {
        // THREE POSITIONS, because only two of them reach the trim: the run is
        // taken off the ENDS of a rendered block, so an interior line survives
        // a Unicode-property trim and the FIRST and LAST lines do not. A
        // fixture that only puts the character mid-paragraph reports a clean
        // run against the defect - which is what the corpus document that
        // caught this one does NOT do, and what this loop would have done.
        for src in [
            format!("a\n{ch}\nb\n"),
            format!("a\n{ch}\n"),
            format!("{ch}\na\n"),
        ] {
            assert!(round_trips(&src), "{name} did not survive fmt in {src:?}");
        }
        // The whole document, where the character is the only thing in it.
        let alone = format!("{ch}\n");
        assert!(round_trips(&alone), "{name} did not survive fmt alone");
    }
}

#[test]
fn a_control_character_is_written_back() {
    // 61 codepoints were dropped from text outright - every C0 control but
    // tab/newline/return, DEL, and the whole C1 block - none of which the
    // parser or the HTML renderer drops.
    for (name, ch) in [
        ("VERTICAL TAB", '\u{b}'),
        ("FORM FEED", '\u{c}'),
        ("NEXT LINE", '\u{85}'),
        ("START OF HEADING", '\u{1}'),
        ("DELETE", '\u{7f}'),
        ("PADDING CHARACTER", '\u{80}'),
        ("APPLICATION PROGRAM COMMAND", '\u{9f}'),
    ] {
        let src = format!("a{ch}b\n");
        assert!(round_trips(&src), "{name} did not survive fmt");
        assert!(
            to_carve(&src).contains(ch),
            "{name} was dropped by the writer"
        );
    }
}

#[test]
fn a_leading_byte_order_mark_is_written_where_a_reparse_can_still_read_it() {
    // `normalize_source` strips a single leading U+FEFF before the parser sees
    // it, so a document whose first content character is one cannot be written
    // flush left - the re-parse eats it and the document comes back empty.
    let src = " \u{feff} \n";
    assert_eq!(to_html(src).trim(), "<p>\u{feff}</p>");
    assert!(round_trips(src), "the mark did not survive fmt");
    // Idempotent: the second pass sees the same tree and writes the same line.
    let once = to_carve(src);
    assert_eq!(to_carve(&once), once, "fmt is not idempotent here");
}

#[test]
fn control_u0000_stays_dropped() {
    // The one the parser drops too: `normalize_source` removes it before the
    // parse, so writing it back would emit a byte no re-parse can read.
    assert!(!to_carve("a\u{0}b\n").contains('\u{0}'));
    assert!(round_trips("a\u{0}b\n"));
}

#[test]
fn control_a_whitespace_only_line_is_still_written_empty() {
    // PART 11 section 7: a line whose only content is ASCII space or tab is
    // emitted EMPTY, wherever it sits - editors and CI that strip trailing
    // whitespace rewrite one, so `fmt` would report a diff on a file nobody
    // edited. Narrowing the terminal must not have re-admitted it.
    let out = to_carve("a\n \nb\n");
    for line in out.lines() {
        assert!(
            line.is_empty() || !line.trim_matches([' ', '\t']).is_empty(),
            "a whitespace-only line was emitted: {out:?}"
        );
    }
}

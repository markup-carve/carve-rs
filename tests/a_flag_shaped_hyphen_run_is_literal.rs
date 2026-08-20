//! PART 9 §8, A FLAG-SHAPED HYPHEN RUN IS LITERAL (markup-carve/carve#1443).
//!
//! A run PRECEDED by whitespace (or the start of the content) and FOLLOWED by a
//! non-whitespace character is a long CLI flag, not a dash. The failure it
//! repairs was silent and output-only: the author saw `git log --oneline` in the
//! source and the reader got a command that does not run.
//!
//! The narrowness is the design. Every canonical dash use is unspaced on at
//! least one side, so a rule keyed on whitespace-both-sides would have removed
//! the feature along with the damage, and one keyed on the two sides matching in
//! kind would have broken `a---- b----- c------`, which the corpus pins.

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_flag_keeps_its_hyphens() {
    assert_eq!(
        html("git log --oneline and --force-with-lease\n"),
        "<p>git log --oneline and --force-with-lease</p>"
    );
    assert_eq!(html("--force x\n"), "<p>--force x</p>");
}

#[test]
fn every_other_position_still_converts() {
    assert_eq!(html("pages 1--10\n"), "<p>pages 1\u{2013}10</p>");
    assert_eq!(
        html("the Mon--Fri window\n"),
        "<p>the Mon\u{2013}Fri window</p>"
    );
    assert_eq!(
        html("a thought---interrupted---resumes\n"),
        "<p>a thought\u{2014}interrupted\u{2014}resumes</p>"
    );
    assert_eq!(html("a -- b\n"), "<p>a \u{2013} b</p>");
    assert_eq!(html("text --\n"), "<p>text \u{2013}</p>");
    assert_eq!(
        html("a---- b----- c------\n"),
        "<p>a\u{2013}\u{2013} b\u{2014}\u{2013} c\u{2014}\u{2014}</p>"
    );
}

#[test]
fn the_run_is_consumed_whole() {
    // Consuming it a hyphen at a time would leave `-->` as a stray `-` plus a
    // live `->` symbol, and the flag would render a rightwards arrow.
    assert_eq!(html("x -->\n"), "<p>x --&gt;</p>");
    assert_eq!(html("x ---foo\n"), "<p>x ---foo</p>");
}

#[test]
fn an_html_comment_is_half_repaired() {
    // A stated limit: the opening run is preceded by `!` rather than whitespace,
    // so it still converts.
    assert_eq!(html("<!-- c -->\n"), "<p>&lt;!\u{2013} c --&gt;</p>");
}

#[test]
fn the_space_class_is_part_sevens() {
    // A VERTICAL TAB and a FORM FEED are CONTENT in Carve, so a run followed by
    // one answers the way a run followed by an ordinary content character
    // answers. `char::is_whitespace` takes both.
    for probe in ['!', '\u{000b}', '\u{000c}'] {
        assert_eq!(
            html(&format!("---{probe}\n")),
            format!("<p>---{probe}</p>"),
            "a content character behaved like a space"
        );
    }
}

#[test]
fn a_no_break_space_is_a_space() {
    // In either of its spellings: the literal character, and the escape.
    assert_eq!(html("a\u{00a0}--foo\n"), "<p>a&nbsp;--foo</p>");
    assert_eq!(html("a\\ --foo\n"), "<p>a&nbsp;--foo</p>");
}

#[test]
fn the_writer_keeps_the_hyphens() {
    let source = "git log --oneline\n";
    let formatted = carve::to_carve(source);

    assert_eq!(formatted, source);
    assert_eq!(html(&formatted), html(source));
}

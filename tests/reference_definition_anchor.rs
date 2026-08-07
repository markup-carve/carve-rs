//! A REFERENCE DEFINITION IS ANCHORED AT END OF LINE (PART 7, carve#911).
//!
//! ```text
//! reference_definition = '[', reference_label, ']', ':', space, link_destination,
//!                        [link_title], [space, attributes], newline ;
//! ```
//!
//! It ends in `newline` and always has. What follows the destination and the
//! optional title makes the production FAIL, and the line is then an ordinary
//! paragraph. All three engines and the executable spec read it as a definition
//! with trailing junk, and nothing in the grammar authorized the reading.
//!
//! WHY IT MATTERS BEYOND TIDINESS. PART 7 promises that a slot which fails to
//! match "falls back to prose rather than silently dropping metadata". At this
//! line there was no prose to fall back to: the swallowing tail ate whatever a
//! failed slot rejected, so the promised failure mode was unreachable here and
//! every narrowing at this line dropped metadata instead of failing visibly.
//! That is why carve#907 deliberately left two shapes unpinned - a mixed run at
//! the definition form of `link_title`, and the `<SP><TAB>` order at the
//! trailing-attributes slot. With the line anchored, both produce the visible
//! failure and both are pinned here.
//!
//! THE LINE ENDING is `whitespace`, a space or a tab - the same terminal
//! `blank_line = {whitespace}` takes (PART 1, carve#890). Implementing it as a
//! Unicode whitespace PROPERTY reads a no-break space, an en quad and an
//! ideographic space as a line ending too, and a plain tab fixture cannot see
//! the difference because a tab is inside the property as well. The corpus
//! cannot see it either: no corpus document ends a definition line with an
//! invisible character, so those rows live here.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn resolves(src: &str) -> bool {
    to_html(src).contains("href=")
}

// ---------------------------------------------------------------------------
// The rule
// ---------------------------------------------------------------------------

#[test]
fn a_trailing_junk_tail_is_not_a_definition() {
    assert_eq!(
        squash(&to_html("[a]: /u zzz\n\n[a][]\n")),
        "<p>[a]: /u zzz</p> <p>[a][]</p>"
    );
}

#[test]
fn a_tail_after_a_title_is_not_a_definition_either() {
    // carve-rs#733's shape: this engine discarded the TITLE and kept the
    // definition, and the executable spec kept the title and ignored the tail.
    // The anchor says neither: the line is a paragraph.
    assert!(!resolves("[a]: /u \"T\" x\n\n[a][]\n"));
    assert!(!resolves("[a]: /u \"T\"\t{.c}\n\n[a][]\n"));
}

#[test]
fn the_tab_narrowing_at_both_slots_follows_automatically() {
    // Each slot carries the tab-first form and BOTH mixed runs. A rule about a
    // run written as "the first character must be a space" passes the tab-first
    // fixture and admits `<SP><TAB>`; written as "the last character must be a
    // space" it admits `<TAB><SP>` instead. Both spellings have been written
    // for real in this org.
    for src in [
        "[a]: /u\t\"T\"\n\n[a][]\n",
        "[a]: /u \t\"T\"\n\n[a][]\n",
        "[a]: /u\t \"T\"\n\n[a][]\n",
        "[a]: /u\t{.c}\n\n[a][]\n",
        "[a]: /u \t{.c}\n\n[a][]\n",
        "[a]: /u\t {.c}\n\n[a][]\n",
    ] {
        assert!(!resolves(src), "still a definition: {src:?}");
    }
}

// ---------------------------------------------------------------------------
// The line ending: a space or a tab, and NOTHING else
// ---------------------------------------------------------------------------

#[test]
fn a_run_of_spaces_and_tabs_is_a_line_ending() {
    for src in [
        "[a]: /u \n\n[a][]\n",
        "[a]: /u\t\n\n[a][]\n",
        "[a]: /u \t \n\n[a][]\n",
    ] {
        assert!(resolves(src), "lost the definition: {src:?}");
    }
}

#[test]
fn an_invisible_character_after_the_destination_is_content_not_a_line_ending() {
    // A no-break space, an en quad, an ideographic space and a form feed are
    // CONTENT under carve#890, so a line carrying one after the destination is
    // not a definition. A Unicode whitespace `trim_end` reads all four as a
    // line ending, and no corpus document can see the difference - which is why
    // this case is here and not there.
    for (name, ch) in [
        ("NO-BREAK SPACE", '\u{a0}'),
        ("EN QUAD", '\u{2000}'),
        ("IDEOGRAPHIC SPACE", '\u{3000}'),
        ("FORM FEED", '\u{c}'),
        ("NEXT LINE", '\u{85}'),
    ] {
        let src = format!("[a]: /u{ch}\n\n[a][]\n");
        assert!(!resolves(&src), "{name} read as a line ending");
    }
}

#[test]
fn a_byte_order_mark_after_the_destination_joins_the_destination() {
    // The row that reads the other way, and it is not an exception to the rule
    // above - it is `unicode_url_char` doing its job. U+FEFF is not whitespace,
    // so it does not END the destination; it is PART of it, and the line is
    // still a definition. Measured against the executable spec, which renders
    // the same href.
    assert_eq!(
        to_html("[a]: /u\u{feff}\n\n[a][]\n").trim(),
        "<p><a href=\"/u\u{feff}\">a</a></p>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS: the over-rejection risk
// ---------------------------------------------------------------------------

#[test]
fn control_every_legal_shape_still_defines() {
    assert_eq!(
        to_html("[a]: /u\n\n[a][]\n").trim(),
        "<p><a href=\"/u\">a</a></p>"
    );
    assert_eq!(
        to_html("[a]: /u \"T\"\n\n[a][]\n").trim(),
        "<p><a href=\"/u\" title=\"T\">a</a></p>"
    );
    assert_eq!(
        to_html("[a]: /u {.c}\n\n[a][]\n").trim(),
        "<p><a href=\"/u\" class=\"c\">a</a></p>"
    );
    assert_eq!(
        to_html("[a]: /u \"T\" {.c}\n\n[a][]\n").trim(),
        "<p><a href=\"/u\" title=\"T\" class=\"c\">a</a></p>"
    );
}

#[test]
fn control_a_glued_brace_run_is_the_destination() {
    // The trap: nothing is left over here, because `link_destination` simply
    // reads the braces. `[a]: /u{.c}` gives `href="/u{.c}"` and IS a
    // definition; `[a]: /u<SP><SP>{.c}` is not, and the two are different
    // shapes rather than two spellings of one.
    assert_eq!(
        to_html("[a]: /u{.c}\n\n[a][]\n").trim(),
        "<p><a href=\"/u{.c}\">a</a></p>"
    );
    assert!(!resolves("[a]: /u  {.c}\n\n[a][]\n"));
}

#[test]
fn control_an_escaped_quote_inside_a_title_does_not_end_it() {
    // The closing quote is found with the escape rule, so a title carrying one
    // still ends where the author ended it - and the line is still anchored.
    assert_eq!(
        to_html("[a]: /u \"a\\\"b\"\n\n[a][]\n").trim(),
        "<p><a href=\"/u\" title=\"a&quot;b\">a</a></p>"
    );
    assert!(!resolves("[a]: /u \"a\\\"b\" x\n\n[a][]\n"));
}

#[test]
fn control_a_run_before_the_destination_is_a_different_question() {
    // carve#911 rules what follows the destination. A run BEFORE it is not this
    // ruling, and the executable spec still reads this as a definition with the
    // destination sanitized rather than refusing the line - so the head stays
    // exactly as it was.
    let html = to_html("[click][a]\n\n[a]: \u{202f}javascript:alert(1)\n");
    assert!(
        html.contains("href=\"\"") || !html.contains("href="),
        "the obfuscated scheme reached an href: {html}"
    );
    assert!(!html.contains("javascript:alert(1)\">"), "{html}");
}

#[test]
fn control_a_footnote_definition_is_a_different_production() {
    // `footnote_definition` is not anchored by this ruling, and its body is
    // inline content that runs to the end of the line by design.
    let html = to_html("x[^f]\n\n[^f]: note with several words\n");
    assert!(html.contains("<p>note with several words<a"), "{html}");
}

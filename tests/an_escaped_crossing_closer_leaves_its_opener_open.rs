//! carve-rs#2211 added the second pairing pass that decides the crossing-bracket
//! escape, and two cases came out of it losing content on import.
//!
//! WHAT WAS WRONG, in one sentence each.
//!
//! carve-rs#2214, a pre-existing case the second pass did not reach: the pass
//! spent an opener on a closer it was about to escape. An escaped `]` answers
//! nothing, so that opener is still unanswered when the next `]` arrives, and the
//! second crossing closer was left bare. On `<p>[[[<ins>]]</ins></p>` the
//! importer wrote `[\[\[{+\]]+}`, which the spec's reader reads as
//! `<p>[[[{+]]+}</p>` - the mark is gone.
//!
//! carve-rs#2215, which carve-rs#2211 introduced: a `]` in BRACKETED content
//! took its escape from whether its own opener was escaped, and never from the
//! run. At the top level `closer_openers` is cleared, so the crossing escape
//! reached the closer there; inside a link label or a span the map survives, the
//! mirror answered first, and the escape never landed. On
//! `<p><a href="b">[[<del>]</del></a></p>` the importer wrote `[[\[{-]-}](b)`,
//! read back as `<p><a href="b">[[{-]-}</a></p>`.
//!
//! WHY NO CHECK SAW EITHER. This engine reads its own output back as the mark it
//! wrote, so the round-trip assertion passed on a spelling the rest of the fleet
//! drops. Every byte below is measured against carve-php at `0cb5a06b`, and
//! against the spec's own reader at the pinned spec carve `aa3678a2`.

use carve::{html_to_carve, parse, render_html, to_carve, HtmlImportOptions};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

/// carve-rs#2214. A second crossing closer takes its own escape, because the
/// opener the first one fell through to was never spent on it.
#[test]
fn a_second_crossing_closer_is_not_left_bare() {
    assert_eq!(imported("<p>[[[<ins>]]</ins></p>"), "[\\[\\[{+\\]\\]+}\n");
    assert_eq!(imported("<p>[[[<del>]]</del></p>"), "[\\[\\[{-\\]\\]-}\n");
    assert_eq!(
        imported("<p>[[[[<ins>]]]</ins></p>"),
        "[\\[\\[\\[{+\\]\\]\\]+}\n"
    );
    assert_eq!(
        imported("<p>[[[[[<del>]]]</del></p>"),
        "[[\\[\\[\\[{-\\]\\]\\]-}\n"
    );
}

/// carve-rs#2215. In a link label and in a span the crossing closer takes the
/// escape the run owes it, rather than mirroring an opener that kept its bare
/// form.
#[test]
fn a_crossing_closer_inside_a_bracketed_scope_takes_its_escape() {
    assert_eq!(
        imported("<p><a href=\"b\">[[<del>]</del></a></p>"),
        "[\\[\\[{-\\]-}](b)\n"
    );
    assert_eq!(
        imported("<p><span class=\"c\">[[<em>]</em></span></p>"),
        "[\\[\\[/\\]/]{.c}\n"
    );
    assert_eq!(
        imported("<p><span class=\"c\">[[[<ins>a]b</ins>]</span></p>"),
        "[\\[[\\[{+a\\]b+}]]{.c}\n"
    );
}

/// The shape that nearly shipped with the carve-rs#2214 half of the fix alone.
/// Leaving the opener open re-pairs the closer onto a bare opener, so the mirror
/// answered `false` where the run owed an escape - both halves are needed, and
/// this is the case that separates them.
#[test]
fn a_trailing_closer_does_not_steal_the_crossing_escape() {
    assert_eq!(
        imported("<p><a href=\"b\">[[[<em>]</em>]</a></p>"),
        "[\\[[\\[/\\]/]](b)\n"
    );
    assert_eq!(
        imported("<p><a href=\"b\">[[<mark>a]b</mark>]]</a></p>"),
        "[[\\[=a\\]b=]\\]](b)\n"
    );
}

/// Every shape the two tickets measured formats to itself and re-reads as the
/// document it came from. The nine hosts, the leading and inner bracket counts,
/// and both bracketed wrappers.
#[test]
fn every_measured_shape_is_idempotent_and_round_trips() {
    let hosts = ["em", "strong", "u", "s", "mark", "ins", "del", "sup", "sub"];
    let mut checked = 0;
    for host in hosts {
        for leading in 2..=5 {
            for inner in 1..=3 {
                let html = format!(
                    "<p>{}<{host}>{}</{host}></p>",
                    "[".repeat(leading),
                    "]".repeat(inner)
                );
                assert_round_trips(&html);
                checked += 1;
            }
        }
        for (open, close) in [
            ("<a href=\"b\">", "</a>"),
            ("<span class=\"c\">", "</span>"),
        ] {
            for leading in ["", "[", "[[", "[[["] {
                for inner in ["]", "]]", "a]", "a]b"] {
                    for trailing in ["", "]", "]]", "x"] {
                        let html = format!(
                            "<p>{open}{leading}<{host}>{inner}</{host}>{trailing}{close}</p>"
                        );
                        assert_round_trips(&html);
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 108 + 1152);
}

fn assert_round_trips(html: &str) {
    let once = imported(html);
    assert_eq!(
        to_carve(&once),
        once,
        "formatting {html} imported as {once:?} is not idempotent"
    );
    assert_eq!(
        render_html(&parse(&once)).unwrap(),
        html,
        "{html} imported as {once:?}"
    );
}

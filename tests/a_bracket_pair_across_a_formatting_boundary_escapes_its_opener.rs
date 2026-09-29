//! Where a `[`...`]` pair crosses a formatting boundary, the writer escapes the
//! OPENING bracket.
//!
//! This is a CANONICAL-FORM decision, not a correctness fix. This engine used to
//! escape the closer instead, and both spellings conform: each spends exactly one
//! backslash, and `\[/a](b)/` and `[/a\](b)/` re-parse to the same tree and
//! render the same HTML. PART 11 CARVE-P11-006 therefore does not choose between
//! them.
//!
//! What chooses is the shared html-import fixture
//! `tests/html-import/paren-after-a-closed-bracket`: it is the cross-engine
//! contract for importer output, byte-identical output across the three engines
//! is what it exists to pin, and carve-php took the opener spelling in
//! markup-carve/carve-php#2756. The rule it applies is the one mirrored here -
//! PART 8 resolves a bracket run before the emphasis markers, so a run that
//! spans a formatting boundary isolates the delimiter inside it, and escaping
//! the opener is what keeps the run from forming at all. Measured against
//! carve-php at `e7589a0f`: every shape below comes out byte-identical there.

use carve::{html_to_carve, parse, render_html, HtmlImportOptions};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

#[test]
fn the_opener_carries_the_escape() {
    assert_eq!(imported("<p>[<em>a](b)</em></p>"), "\\[/a](b)/\n");
}

/// Both spellings hold, which is why the fixture rather than the re-parse
/// settles which one is written.
#[test]
fn the_two_spellings_render_the_same_html() {
    let expected = "<p>[<em>a](b)</em></p>";
    assert_eq!(render_html(&parse("\\[/a](b)/\n")).unwrap(), expected);
    assert_eq!(render_html(&parse("[/a\\](b)/\n")).unwrap(), expected);
}

/// Without the escape the bracket run forms and swallows the opening `/`, so one
/// backslash is still load bearing.
#[test]
fn the_bare_form_loses_the_emphasis() {
    assert_eq!(
        render_html(&parse("[/a](b)/\n")).unwrap(),
        "<p><a href=\"b\">/a</a>/</p>"
    );
}

/// A crossing pair with no destination behind it takes the same escape: the run
/// would isolate the delimiter whether or not a `(` follows.
#[test]
fn a_crossing_pair_without_a_destination_escapes_its_opener_too() {
    assert_eq!(imported("<p>[<em>a]</em></p>"), "\\[/a]/\n");
    assert_eq!(imported("<p>x [<em>a]</em> y</p>"), "x \\[/a]/ y\n");
}

/// A crossing pair nested in a literal pair spends two escapes: with the inner
/// opener escaped, the inner closer answers the OUTER opener, so it needs one
/// too. `[[/a\]/]` holds with one and is what this engine wrote before, which
/// makes the extra backslash a real cost of the rule rather than a free choice.
/// It is pinned because carve-php writes the same bytes here, so trimming it is
/// a fleet decision and not a local one.
#[test]
fn a_crossing_pair_nested_in_a_literal_pair_escapes_both() {
    assert_eq!(imported("<p>[[<em>a]</em>]</p>"), "[\\[/a\\]/]\n");
}

/// CONTROL - a pair whose brackets sit under ONE host crosses nothing, so the
/// escape stays on the paren the destination would open with.
#[test]
fn a_pair_inside_one_host_still_escapes_the_paren() {
    assert_eq!(imported("<p>[a](b)</p>"), "[a]\\(b)\n");
    assert_eq!(imported("<p>[a<em>x</em>b](c)</p>"), "[a{/x/}b]\\(c)\n");
    assert_eq!(imported("<p>[<em>a</em>](b)</p>"), "[/a/]\\(b)\n");
}

/// Every shape above comes back as the HTML that wrote it.
#[test]
fn each_shape_round_trips() {
    for html in [
        "<p>[<em>a](b)</em></p>",
        "<p>[[<em>a]</em>]</p>",
        "<p>[<em>a]</em></p>",
        "<p>x [<em>a]</em> y</p>",
        "<p>[a](b)</p>",
        "<p>[a<em>x</em>b](c)</p>",
        "<p>[<em>a</em>](b)</p>",
    ] {
        let carve = imported(html);
        assert_eq!(
            render_html(&parse(&carve)).unwrap(),
            html,
            "{html} imported as {carve:?}"
        );
    }
}

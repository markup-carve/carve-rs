//! carve-rs#2206 put the escape on the opener of a bracket pair that crosses a
//! formatting boundary. It decided that in ONE forward pass, and one pass cannot
//! decide it: escaping an opener removes it from the run, so the `]` that
//! answered it falls through to the outer opener, and where THAT pair crosses the
//! same boundary a second escape is owed. Nothing paid it, so `fmt` computed it
//! itself and wrote one more backslash on every run.
//!
//! WHAT THAT LOOKED LIKE, on `<p>[[<ins>a]</ins>]</p>`:
//!
//! ```text
//! import  [\[{+a]+}]
//! fmt^1   \[\[{+a]+}]
//! fmt^2   \[\[{+a]+}]
//! ```
//!
//! It converged, one step per bracket level, so it was bounded rather than
//! runaway. It still meant the importer's own output failed `carve fmt --check`,
//! and three levels of bracket took three passes to settle.
//!
//! THE SHAPE OF THE FIX is the pass that was missing, not a new rule: pair the
//! run, name the crossing openers, then pair it AGAIN with those openers gone.
//! A pair that still crosses in the second reading has its CLOSER escaped, which
//! is terminal because the outer opener keeps its bare form. Every byte below was
//! measured against carve-php at `e7589a0f`, which already wrote the second
//! escape - this closes the only gap between the two writers on these shapes.

use carve::{html_to_carve, parse, render_html, to_carve, HtmlImportOptions};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

/// THE DEFECT. Every shape formats to itself, and re-reads as the HTML it came
/// from. The first assertion is the one that failed.
#[test]
fn every_crossing_shape_is_idempotent_and_round_trips() {
    for html in [
        "<p>[<ins>a]</ins></p>",
        "<p>[[<ins>a]</ins>]</p>",
        "<p>[[[<ins>a]</ins>]]</p>",
        "<p>[<ins>a](b)</ins></p>",
        "<p>[<ins>a]</ins>]</p>",
        "<p>[<del>a]</del></p>",
        "<p>[[<del>a]</del>]</p>",
        "<p>[[[<del>a]</del>]]</p>",
        "<p>[<sup>a]</sup></p>",
        "<p>[[<sup>a]</sup>]</p>",
        "<p>[[[<sup>a]</sup>]]</p>",
        "<p>[<sub>a]</sub></p>",
        "<p>[[<sub>a]</sub>]</p>",
        "<p>[<em>a]</em></p>",
        "<p>[[<em>a]</em>]</p>",
        "<p>[<strong>a]</strong></p>",
        "<p>[[<strong>a]</strong>]</p>",
        "<p>[<mark>a]</mark></p>",
        "<p>[[<mark>a]</mark>]</p>",
        "<p>[<u>a]</u></p>",
        "<p>[<s>a]</s></p>",
        "<p>[<sup><ins>a]</ins></sup></p>",
        "<p>[<ins><sup>a]</sup></ins></p>",
        "<p>[[<sup>a]</sup><ins>b]</ins>]</p>",
        "<p>[<ins>a]</ins><del>b</del></p>",
    ] {
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
}

/// The critic family is where the missing pass showed, because a critic mark is
/// the one host whose content still reads back through the run - so the shape
/// survived the render check that would have caught an emphasis.
#[test]
fn a_crossing_pair_nested_in_a_literal_pair_escapes_its_closer() {
    assert_eq!(imported("<p>[[<ins>a]</ins>]</p>"), "[\\[{+a\\]+}]\n");
    assert_eq!(imported("<p>[[<del>a]</del>]</p>"), "[\\[{-a\\]-}]\n");
    assert_eq!(imported("<p>[[[<ins>a]</ins>]]</p>"), "[[\\[{+a\\]+}]]\n");
}

/// Where no outer opener survives to answer it, the closer keeps its bare form:
/// one escape, on the opener, exactly as carve-rs#2206 wrote it.
#[test]
fn a_lone_crossing_pair_still_spends_one_escape() {
    assert_eq!(imported("<p>[<ins>a]</ins></p>"), "\\[{+a]+}\n");
    assert_eq!(imported("<p>[<sup>a]</sup></p>"), "\\[{^a]^}\n");
    assert_eq!(imported("<p>[<em>a](b)</em></p>"), "\\[/a](b)/\n");
    assert_eq!(imported("<p>[<em>a]</em></p>"), "\\[/a]/\n");
}

/// TWO crossing pairs in one run leave no surviving opener either, so both spend
/// one escape and no closer takes one. This is the case a rule keyed on "the
/// stack is not empty" gets wrong, which is why the second reading is a pairing
/// pass rather than a depth test.
#[test]
fn two_crossing_pairs_escape_two_openers_and_no_closer() {
    assert_eq!(
        imported("<p>[[<sup>a]</sup><ins>b]</ins>]</p>"),
        "\\[\\[{^a]^}{+b]+}]\n"
    );
}

/// CONTROL - a pair inside one host crosses nothing in either reading, so the
/// escape stays on the paren the destination would open with.
#[test]
fn a_pair_inside_one_host_is_untouched() {
    assert_eq!(imported("<p>[a](b)</p>"), "[a]\\(b)\n");
    assert_eq!(imported("<p>[a<em>x</em>b](c)</p>"), "[a{/x/}b]\\(c)\n");
    assert_eq!(imported("<p>[<em>a</em>](b)</p>"), "[/a/]\\(b)\n");
    assert_eq!(imported("<p>[<ins>a</ins>](b)</p>"), "[{+a+}]\\(b)\n");
}

/// WHY THE RENDER CHECKS COULD NOT SEE THIS. A critic mark reads back through
/// the run, so all three spellings of the nested shape render alike and only the
/// writer's own agreement separates them. The superscript host is the opposite
/// case: the run isolates its delimiter, the render changes, and that is why the
/// emphasis families were already spelled the way carve-php spells them.
#[test]
fn the_three_critic_spellings_render_alike() {
    for src in ["[[{+a]+}]\n", "[\\[{+a]+}]\n", "[\\[{+a\\]+}]\n"] {
        assert_eq!(
            render_html(&parse(src)).unwrap(),
            "<p>[[<ins>a]</ins>]</p>",
            "{src:?}"
        );
    }
    assert_eq!(render_html(&parse("[{^a]^}\n")).unwrap(), "<p>[{^a]^}</p>");
}

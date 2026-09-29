//! PART 8 resolves a bracket run at rank 5 and the critic family at rank 8, so
//! the run wins the same way it wins against a bare marker: an `X}` inside a run
//! is that run's content and closes nothing opened outside it
//! (markup-carve/carve#2577 for the bare form, carve-rs#2212 for this one).
//!
//! carve-rs let `{+ +}` and `{- -}` cross the boundary while refusing it for
//! every forced emphasis spelling, which made the rule read as a property of the
//! delimiter rather than of the run.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

/// The three spellings that crossed the boundary before the fix.
#[test]
fn a_critic_closer_outside_the_openers_run_closes_nothing() {
    assert_eq!(html("[{+a]+}\n"), "<p>[{+a]+}</p>");
    assert_eq!(html("[{-a]-}\n"), "<p>[{-a]-}</p>");
    assert_eq!(html("[{~a]~>b~}\n"), "<p>[{~a]~&gt;b~}</p>");
}

/// The forced emphasis spellings, which already read the rule. Listed here so
/// the family is pinned as ONE rule rather than as two neighboring ones.
#[test]
fn every_braced_spelling_reads_the_same_way() {
    assert_eq!(html("[{/a]/}\n"), "<p>[{/a]/}</p>");
    assert_eq!(html("[{*a]*}\n"), "<p>[{*a]*}</p>");
    assert_eq!(html("[{_a]_}\n"), "<p>[{_a]_}</p>");
    assert_eq!(html("[{^a]^}\n"), "<p>[{^a]^}</p>");
    assert_eq!(html("[{,a],}\n"), "<p>[{,a],}</p>");
    assert_eq!(html("[{=a]=}\n"), "<p>[{=a]=}</p>");
    assert_eq!(html("[{~a]~}\n"), "<p>[{~a]~}</p>");
}

/// The bare spelling of the same rule, which is what makes the braced one an
/// inconsistency rather than a second decision.
#[test]
fn the_bare_spelling_reads_the_same_way() {
    assert_eq!(html("[/a]/\n"), "<p>[/a]/</p>");
    assert_eq!(html("[+a]+\n"), "<p>[+a]+</p>");
}

/// Both halves inside one run still pair, and a mark outside every run is
/// untouched: the bound is sharing a run, not the presence of a bracket.
#[test]
fn a_pair_that_shares_its_run_still_pairs() {
    assert_eq!(html("[{+a+}]\n"), "<p>[<ins>a</ins>]</p>");
    assert_eq!(html("[{-a-}]\n"), "<p>[<del>a</del>]</p>");
    assert_eq!(html("[{~a~>b~}]\n"), "<p>[<del>a</del><ins>b</ins>]</p>");
    assert_eq!(html("{+a+}\n"), "<p><ins>a</ins></p>");
    assert_eq!(html("{~a~>b~}\n"), "<p><del>a</del><ins>b</ins></p>");
}

/// A run whose `]` is followed by a link control reads the same way: the mark's
/// closer is still in another run, and the `(` cannot select a construct for a
/// run that ends inside the mark. Matches carve-php byte for byte.
#[test]
fn a_link_control_after_the_run_changes_nothing() {
    assert_eq!(html("[{+a]+}(/u)\n"), "<p>[{+a]+}(/u)</p>");
    assert_eq!(html("{+[a]+}(/u)\n"), "<p><ins>[a]</ins>(/u)</p>");
}

/// The escaped spelling the writers emit is unaffected: with the opener escaped
/// no run forms, so the mark keeps its content across what would have been one
/// (carve-rs#2209, #2211).
#[test]
fn an_escaped_opening_bracket_keeps_the_mark() {
    assert_eq!(html("\\[{+a]+}\n"), "<p>[<ins>a]</ins></p>");
}

/// The braced en dash is a spelling of its own and takes no closer, so the run
/// cannot bound it.
#[test]
fn the_braced_en_dash_is_unaffected() {
    assert_eq!(html("[{--}]\n"), "<p>[\u{2013}]</p>");
}

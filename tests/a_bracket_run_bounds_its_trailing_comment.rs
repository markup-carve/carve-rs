//! `CARVE-P9-041` gives a trailing `%%` marker the rest of its line, and the
//! line is bounded by the inline run hosting the marker: a bracket run ends at
//! its own closer (`markup-carve/carve#2576`, carve-rs#2160).
//!
//! The deciding shape is not a bare run but a container whose label is valid.
//! `::: note [a %% x]` renders the container; if the comment took every
//! character to the line break it would take the `]`, the opener would never
//! complete, and the container plus every child it holds would be lost. So the
//! run's closer bounds the comment, and the run's START is where the clause asks
//! for no separator before the marker.

use carve::to_html;

/// The bug: the closer went with the comment, and so did everything after it.
#[test]
fn a_bracket_runs_closer_survives_a_comment_inside_it() {
    assert_eq!(to_html("[a %% hidden]\n").trim(), "<p>[a]</p>");
    assert_eq!(to_html("[a\t%% hidden]\n").trim(), "<p>[a]</p>");
    assert_eq!(to_html("[a %% hidden] tail\n").trim(), "<p>[a] tail</p>");
    assert_eq!(
        to_html("pre [a %% hidden] post\n").trim(),
        "<p>pre [a] post</p>"
    );
    assert_eq!(
        to_html("[a %% h1] and [b %% h2]\n").trim(),
        "<p>[a] and [b]</p>"
    );
}

/// The run that bounds the comment is the INNERMOST one holding it.
#[test]
fn the_innermost_run_bounds_the_comment() {
    assert_eq!(to_html("[see [t] %% hidden]\n").trim(), "<p>[see [t]]</p>");
    assert_eq!(
        to_html("[see [t %% hidden] x]\n").trim(),
        "<p>[see [t] x]</p>"
    );
}

/// A run's start needs no separator before the marker, exactly as the start of a
/// whole line does - corpus 518's second property, one host further in.
#[test]
fn a_marker_starting_a_bracket_run_needs_no_separator() {
    assert_eq!(to_html("[%% hidden]\n").trim(), "<p>[]</p>");
    assert_eq!(to_html("[%%hidden]\n").trim(), "<p>[]</p>");
    assert_eq!(to_html("[ %% hidden]\n").trim(), "<p>[]</p>");
}

/// No separator is still no comment, inside a run as anywhere else.
#[test]
fn a_run_without_a_separator_keeps_its_percent_signs() {
    assert_eq!(to_html("[a%% hidden]\n").trim(), "<p>[a%% hidden]</p>");
}

/// A run with no closer has no bound but the line break, which is the reading
/// that was already right.
#[test]
fn an_unclosed_run_leaves_the_comment_its_whole_line() {
    assert_eq!(to_html("[a %% hidden\n").trim(), "<p>[a</p>");
    assert_eq!(to_html("a %% hidden\n").trim(), "<p>a</p>");
}

/// The bound is the closer OR the line break, whichever comes first: a comment
/// never reaches the next line because a run spans one.
#[test]
fn a_line_break_still_bounds_a_comment_inside_a_run() {
    assert_eq!(to_html("[a %% hidden\nb]\n").trim(), "<p>[a\nb]</p>");
}

/// What is opaque to the marker stays opaque. The run's closer is read by the
/// pipeline's one bracket scanner, which already hides an escape and a verbatim
/// span, so nothing here enumerates a construct.
#[test]
fn an_opaque_construct_inside_a_run_is_not_a_marker() {
    assert_eq!(
        to_html("[a `%% no` %% yes]\n").trim(),
        "<p>[a <code>%% no</code>]</p>"
    );
    assert_eq!(to_html("[a \\] b %% hidden]\n").trim(), "<p>[a ] b]</p>");
}

/// The closer being reachable again is what lets a delimiter after it close.
/// The emphasis closing scan keeps its own copy of the extent rule, so this is
/// the shape that fails when only the inline loop is fixed.
#[test]
fn a_delimiter_after_the_runs_closer_still_closes() {
    assert_eq!(
        to_html("*[a %% hidden]*\n").trim(),
        "<p><strong>[a]</strong></p>"
    );
    assert_eq!(to_html("_[a %% hidden]_\n").trim(), "<p><u>[a]</u></p>");
    assert_eq!(
        to_html("*[a %% hidden]* tail\n").trim(),
        "<p><strong>[a]</strong> tail</p>"
    );
    // Without a run to bound it the comment still takes the closer, which is
    // the reading the oracle gives too.
    assert_eq!(to_html("*a %% hidden*\n").trim(), "<p>*a</p>");
}

/// The container that decides the direction. A comment that took the `]` would
/// leave no opener and no children.
#[test]
fn a_valid_container_label_keeps_its_container() {
    let html = to_html("::: note [a\t%% hidden]\nx\n:::\n");
    assert!(html.contains("<p class=\"div-label\">a</p>"), "{html}");
    assert!(html.contains("<p>x</p>"), "{html}");
}

/// A degraded opener is a paragraph, and the paragraph keeps the bracket.
#[test]
fn a_degraded_container_opener_keeps_its_bracket() {
    assert_eq!(
        to_html("::: {.box} [a\t%% hidden]\n").trim(),
        "<p>::: {.box} [a]</p>"
    );
}

/// A run that resolves into a construct was already bounded, because its label
/// is parsed on its own slice. These are the readings that must not move.
#[test]
fn a_resolved_run_reads_as_it_did() {
    assert_eq!(
        to_html("[a %% hidden](u)\n").trim(),
        "<p><a href=\"u\">a</a></p>"
    );
    assert_eq!(
        to_html("[%% hidden](u)\n").trim(),
        "<p><a href=\"u\"></a></p>"
    );
    assert_eq!(
        to_html("[a %% hidden]{.c}\n").trim(),
        "<p><span class=\"c\">a</span></p>"
    );
    assert_eq!(
        to_html("[see [t](u) %% hidden]\n").trim(),
        "<p>[see <a href=\"u\">t</a>]</p>"
    );
    assert_eq!(
        to_html("![a %% hidden](u)\n").trim(),
        "<img src=\"u\" alt=\"a %% hidden\">"
    );
    assert_eq!(to_html("[^a %% hidden]\n").trim(), "<p>[^a %% hidden]</p>");
    assert_eq!(
        to_html("[a %% hidden][r]\n").trim(),
        "<p>[a %% hidden][r]</p>"
    );
}

/// Every host reaches the same reader, so a run inside one is bounded there too.
#[test]
fn every_block_host_bounds_a_runs_comment() {
    for (source, needle) in [
        ("# [a %% hidden]\n", "<h1>[a]</h1>"),
        ("> [a %% hidden]\n", "<p>[a]</p>"),
        ("- [a %% hidden]\n", "<li>[a]</li>"),
        (
            "| h | i |\n|---|---|\n| [x %% hidden] | y |\n",
            "<td>[x]</td>",
        ),
    ] {
        let html = to_html(source);
        assert!(html.contains(needle), "{source:?} -> {html}");
    }
}

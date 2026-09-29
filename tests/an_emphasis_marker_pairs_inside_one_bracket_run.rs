//! PART 8 resolves a bracket run at rank 5 and an emphasis marker at rank 7, so
//! the run wins: a marker inside one is that run's content by the time emphasis
//! is scanned, and cannot answer an opener outside it (markup-carve/carve#2577,
//! corpus category 522, carve-rs#2161).
//!
//! Stated as a pair rule rather than as "a link label is opaque", because the
//! oracle refuses it for a run that resolves into nothing at all. The bound is
//! the run, and a link is one thing a run can become.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

/// Corpus 522, all three rows. The two controls are what isolate the defect to
/// the marker-beside-bracket shape.
#[test]
fn the_corpus_rows() {
    assert_eq!(html("/[a/](/u)\n"), "<p>/<a href=\"/u\">a/</a></p>");
    assert_eq!(html("/a/\n"), "<p><em>a</em></p>");
    assert_eq!(html("/[a](/u)/\n"), "<p><em><a href=\"/u\">a</a></em></p>");
}

/// Every bare delimiter, since the rank is the marker's and not the character's.
#[test]
fn every_bare_delimiter_reads_the_same_way() {
    assert_eq!(html("*[a*](/u)\n"), "<p>*<a href=\"/u\">a*</a></p>");
    assert_eq!(html("_[a_](/u)\n"), "<p>_<a href=\"/u\">a_</a></p>");
    assert_eq!(html("~[a~](/u)\n"), "<p>~<a href=\"/u\">a~</a></p>");
    assert_eq!(html("=[a=](/u)\n"), "<p>=<a href=\"/u\">a=</a></p>");
}

/// The rule is the RUN, not the link: a run that resolves into nothing keeps its
/// marker just the same, which is why this is not stated as link opacity.
#[test]
fn a_run_that_resolves_into_nothing_still_keeps_its_marker() {
    assert_eq!(html("/[a/]\n"), "<p>/[a/]</p>");
    assert_eq!(html("/[a/][r]\n"), "<p>/[a/][r]</p>");
}

/// Every construct a run can become bounds its marker the same way.
#[test]
fn every_resolved_run_bounds_its_marker() {
    assert_eq!(html("/[a/]{.c}\n"), "<p>/<span class=\"c\">a/</span></p>");
    assert_eq!(
        html("/[a/][r]\n\n[r]: /u\n"),
        "<p>/<a href=\"/u\">a/</a></p>"
    );
    assert_eq!(html("/![a/](/u)\n"), "<p>/<img src=\"/u\" alt=\"a/\"></p>");
    assert_eq!(
        html("/[a/](/u \"t\")\n"),
        "<p>/<a href=\"/u\" title=\"t\">a/</a></p>"
    );
}

/// Both halves inside one run still pair - the rule is sharing a run, not the
/// absence of one.
#[test]
fn a_pair_inside_one_run_still_pairs() {
    assert_eq!(html("[a /b/ c]\n"), "<p>[a <em>b</em> c]</p>");
    assert_eq!(
        html("[a /b/ c](/u)\n"),
        "<p><a href=\"/u\">a <em>b</em> c</a></p>"
    );
    assert_eq!(
        html("[a /b/ c /d/ e]\n"),
        "<p>[a <em>b</em> c <em>d</em> e]</p>"
    );
    assert_eq!(html("[/a/](/u)\n"), "<p><a href=\"/u\"><em>a</em></a></p>");
}

/// A run BETWEEN the two halves is passed over, so a pair around one pairs.
#[test]
fn a_run_between_the_halves_does_not_break_the_pair() {
    assert_eq!(html("/a [b] c/\n"), "<p><em>a [b] c</em></p>");
    assert_eq!(
        html("/x [a](/u) y/\n"),
        "<p><em>x <a href=\"/u\">a</a> y</em></p>"
    );
    assert_eq!(
        html("/[a](/u) b/\n"),
        "<p><em><a href=\"/u\">a</a> b</em></p>"
    );
}

/// Half in and half out, in either direction, does not pair.
#[test]
fn a_pair_straddling_a_runs_boundary_does_not_pair() {
    assert_eq!(html("[a /b] c/\n"), "<p>[a /b] c/</p>");
    assert_eq!(html("/a [b/ c]\n"), "<p>/a [b/ c]</p>");
    assert_eq!(html("/a [b/ c](/u)\n"), "<p>/a <a href=\"/u\">b/ c</a></p>");
}

/// The INNERMOST run is what the two halves have to share, and its identity
/// rather than its depth: two sibling runs sit at the same depth, so a depth
/// comparison pairs `[a /b] [c d/]` across the gap between them.
#[test]
fn the_shared_run_is_the_innermost_one_by_identity() {
    assert_eq!(html("[x [a /b/ c] y]\n"), "<p>[x [a <em>b</em> c] y]</p>");
    assert_eq!(html("[x [a /b] c/ y]\n"), "<p>[x [a /b] c/ y]</p>");
    assert_eq!(html("[a /b] [c d/]\n"), "<p>[a /b] [c d/]</p>");
    assert_eq!(html("[a /b] [c d/] e\n"), "<p>[a /b] [c d/] e</p>");
}

/// A nested link inside the label is still the label's run for a marker beside
/// it.
#[test]
fn a_nested_link_in_the_label_keeps_the_labels_run() {
    assert_eq!(
        html("/[see [t](/v)/](/u)\n"),
        "<p>/<a href=\"/u\">see t/</a></p>"
    );
}

/// A destination stays opaque as it was: E2a is a separate rule and this one
/// does not replace it.
#[test]
fn a_destination_is_still_opaque() {
    assert_eq!(html("~[a](b~) c\n"), "<p>~<a href=\"b~\">a</a> c</p>");
    assert_eq!(
        html("/see [x](http://a.b/c) now/\n"),
        "<p><em>see <a href=\"http://a.b/c\">x</a> now</em></p>"
    );
    assert_eq!(html("~[a](b c~) d~\n"), "<p><s>[a](b c</s>) d~</p>");
}

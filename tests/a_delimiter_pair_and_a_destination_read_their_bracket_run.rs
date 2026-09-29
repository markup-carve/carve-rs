//! Two parser defects that share the bracket run, and one leave-one-out control
//! each so a green run isolates them.
//!
//! carve-rs#2174: an angle-wrapped destination was refused by the closing scan on
//! the premise that it could not be a destination. Carve takes it literally, so it
//! is one, and a marker inside it closed there.
//!
//! carve-rs#2173: the combined `/*` scan and the braced `{X` family still paired
//! across a bracket run after #2161 bounded the bare markers. PART 8 resolves a
//! run at rank 5 and an emphasis marker at rank 7, so a closer inside a run is
//! that run's content (markup-carve/carve#2577).

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

// ---------------------------------------------------------------- carve-rs#2174

/// The href is `<b~>`, brackets and all, so the destination is opaque under E2a
/// and the marker inside it closes nothing.
#[test]
fn issue_2174_an_angle_wrapped_destination_is_opaque() {
    assert_eq!(
        html("~[a](<b~>) c\n"),
        "<p>~<a href=\"&lt;b~&gt;\">a</a> c</p>"
    );
    assert_eq!(
        html("~[a](<b(c~)d>) e\n"),
        "<p>~<a href=\"&lt;b(c~)d&gt;\">a</a> e</p>"
    );
}

/// The shapes that genuinely cannot be a destination stay transparent, which is
/// why dropping the refusal on `<` alone is enough: the rules beside it still
/// answer for a space outside a title, an empty destination and an unclosed one.
#[test]
fn issue_2174_a_destination_that_cannot_be_one_is_still_transparent() {
    assert_eq!(html("~[a](<b ~>) c\n"), "<p>~[a](&lt;b ~&gt;) c</p>");
    assert_eq!(html("~[a]() c~\n"), "<p><s>[a]() c</s></p>");
    assert_eq!(html("[a](<b>) c\n"), "<p><a href=\"&lt;b&gt;\">a</a> c</p>");
}

// ---------------------------------------------------------------- carve-rs#2173

/// The combined bold-italic pair, which reaches its closer through `find_seq`
/// rather than through the scan #2161 bounded.
#[test]
fn issue_2173_the_combined_pair_reads_its_run() {
    assert_eq!(html("/*[a*/](/u)\n"), "<p>/*<a href=\"/u\">a*/</a></p>");
    assert_eq!(
        html("[/*a*/](/u)\n"),
        "<p><a href=\"/u\"><strong><em>a</em></strong></a></p>"
    );
    assert_eq!(
        html("/*[a](/u)*/\n"),
        "<p><strong><em><a href=\"/u\">a</a></em></strong></p>"
    );
}

/// Every braced family, since the rule is the run's and not the delimiter's.
#[test]
fn issue_2173_every_braced_family_reads_its_run() {
    for (source, expected) in [
        ("{*[a*}](/u)\n", "<p>{*<a href=\"/u\">a*}</a></p>"),
        ("{/[a/}](/u)\n", "<p>{/<a href=\"/u\">a/}</a></p>"),
        ("{_[a_}](/u)\n", "<p>{_<a href=\"/u\">a_}</a></p>"),
        ("{~[a~}](/u)\n", "<p>{~<a href=\"/u\">a~}</a></p>"),
        ("{=[a=}](/u)\n", "<p>{=<a href=\"/u\">a=}</a></p>"),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

/// Both halves inside one run still pair, and a run between them is passed over.
/// The rule is sharing a run, not the absence of one.
#[test]
fn issue_2173_a_pair_inside_one_run_still_pairs() {
    assert_eq!(
        html("[{*a*} b](/u)\n"),
        "<p><a href=\"/u\"><strong>a</strong> b</a></p>"
    );
    assert_eq!(
        html("{*[a](/u)*}\n"),
        "<p><strong><a href=\"/u\">a</a></strong></p>"
    );
    assert_eq!(
        html("[/*a*/ b](/u)\n"),
        "<p><a href=\"/u\"><strong><em>a</em></strong> b</a></p>"
    );
    assert_eq!(
        html("/*[a](/u) b*/\n"),
        "<p><strong><em><a href=\"/u\">a</a> b</em></strong></p>"
    );
}

/// With no run at all nothing changes, which is the control that says the rule
/// fires on the run rather than on the construct.
#[test]
fn issue_2173_a_pair_with_no_run_is_untouched() {
    assert_eq!(html("/*a*/\n"), "<p><strong><em>a</em></strong></p>");
    assert_eq!(html("{*a*}\n"), "<p><strong>a</strong></p>");
    assert_eq!(html("{^a^}\n"), "<p><sup>a</sup></p>");
    assert_eq!(html("{,a,}\n"), "<p><sub>a</sub></p>");
    assert_eq!(html("{+a+}\n"), "<p><ins>a</ins></p>");
}

// ------------------------------------------------- the run table serves all four

/// The four scans that ask which run holds a position read one table, so the
/// readings #2160 and #2161 established still hold. A stack per scan was four
/// chances to hide a different construct, and the table is built by the pass that
/// pairs the brackets, so it hides exactly what that pass hides.
#[test]
fn the_earlier_run_readings_still_hold() {
    // #2160, the comment extent.
    assert_eq!(html("[a %% hidden]\n"), "<p>[a]</p>");
    assert_eq!(html("[a %% hidden] tail\n"), "<p>[a] tail</p>");
    assert_eq!(html("[%% hidden]\n"), "<p>[]</p>");
    assert_eq!(html("[a %% hidden\n"), "<p>[a</p>");
    // #2161, the bare marker pair.
    assert_eq!(html("/[a/](/u)\n"), "<p>/<a href=\"/u\">a/</a></p>");
    assert_eq!(html("[a /b] [c d/]\n"), "<p>[a /b] [c d/]</p>");
    assert_eq!(html("/a [b] c/\n"), "<p><em>a [b] c</em></p>");
    // An UNCLOSED `[` delimits nothing, so it opens no run for either.
    assert_eq!(html("[a /b/ c\n"), "<p>[a <em>b</em> c</p>");
    // A `[` inside a verbatim span opens none either, which is what building the
    // table inside the pairing pass buys.
    assert_eq!(html("`[` /a/\n"), "<p><code>[</code> <em>a</em></p>");
}

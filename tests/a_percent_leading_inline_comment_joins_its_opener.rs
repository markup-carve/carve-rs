//! A percent-leading inline comment content JOINS its opener
//! (`markup-carve/carve-rs#1589`).
//!
//! PART 11 §2 [CARVE-P11-006] states THE UNIT IS THE OPENER, and §2a
//! [CARVE-P11-008] names this exact rewrite as one a writer must not make:
//!
//!     source     written as    what changed
//!     `| %%%`    `| %% %`      a comment-block fence (§28) split into a line
//!                              comment plus text
//!
//! The inline arm wrote `%% {content}` unconditionally, so a comment whose
//! content is `%` came back as `%% %` - half of a three-character opener run
//! plus a stray character. Both spellings re-parse to the content `%`, so §1's
//! `to_html(fmt(x)) == to_html(x)` never saw it; §2a says so itself, that the
//! invariant holding is necessary and not sufficient.
//!
//! THE BLOCK ARM IS DELIBERATELY NOT THE SAME, and must not be "tidied" into
//! agreement. At block level the comment-LINE marker is exactly `%%`; a run of
//! three or more is a comment FENCE (PART 9 §28) matched on exact width. So the
//! separator there is the only thing keeping a written line a line, and joining
//! it pairs two comment lines into one comment BLOCK that swallows everything
//! between them. `block_comment_lines_keep_their_separator` below is that
//! counterexample, pinned. carve-js declined the same change for the same
//! reason at `markup-carve/carve-js#1675`.

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// PART 11 §1, on the TREE rather than the HTML: `to_html(fmt(x)) == to_html(x)`
/// is what let the defect through, so the assertion is the stronger one.
fn round_trips(src: &str) {
    let out = fmt(src);
    assert_eq!(
        carve::parse(&out).children,
        carve::parse(src).children,
        "parse(fmt(x)) != parse(x)\n  source: {src:?}\n  fmt:    {out:?}"
    );
}

/// The shape the ticket is about, at every inline position that reaches it.
#[test]
fn a_percent_leading_content_joins_the_inline_marker() {
    for (src, want) in [
        ("x %%%\n", "x %%%\n"),
        ("x %% %\n", "x %%%\n"),
        ("x %%%foo\n", "x %%%foo\n"),
        ("x %% %foo\n", "x %%%foo\n"),
        ("x %%%%\n", "x %%%%\n"),
        ("*a* %%%\n", "*a* %%%\n"),
        ("> x %%%\n", "> x %%%\n"),
        ("- - x %%%\n", "- - x %%%\n"),
        ("::: |\na %%%\nb\n:::\n", "::: |\na %%%\nb\n:::\n"),
        ("::: |\na\n%%%\nb\n:::\n", "::: |\na\n%%%\nb\n:::\n"),
        ("::: |\n%%%\n:::\n", "::: |\n%%%\n:::\n"),
    ] {
        assert_eq!(fmt(src), want, "source: {src:?}");
        round_trips(src);
    }
}

/// A content that does NOT open with a percent keeps the separator. The join is
/// keyed on the content, not on the arm.
#[test]
fn a_content_that_opens_with_anything_else_keeps_its_separator() {
    for (src, want) in [
        ("x %% note\n", "x %% note\n"),
        ("x %% a%\n", "x %% a%\n"),
        ("x %%\n", "x %%\n"),
    ] {
        assert_eq!(fmt(src), want, "source: {src:?}");
        round_trips(src);
    }
}

/// THE HAZARD, pinned as its own case. Two comment LINES with the content `%`
/// around a paragraph. Joining them at block level pairs the two runs, makes
/// `x` comment body, and renders the empty string.
#[test]
fn block_comment_lines_keep_their_separator() {
    let src = "%%%\nx\n%% %\n";
    assert_eq!(carve::to_html(src), "<p>x</p>");
    assert_eq!(
        fmt(src),
        "%% %\n\nx\n\n%% %\n",
        "the block arm must not join - see the module docs"
    );
    // The joined spelling is a different document, which is the whole point.
    assert_eq!(carve::to_html("%%%\n\nx\n\n%%%\n"), "");
    round_trips(src);
}

/// The delimited sibling, checked rather than assumed: a percent at any
/// position inside `{% ... %}` round-trips, so it needs no join.
#[test]
fn the_delimited_arms_need_no_join() {
    for src in [
        "x {% % %}\n",
        "x {% %% %}\n",
        "x {% a% %}\n",
        "x {% %a %}\n",
        "{% % %}\n",
        "{% a% %}\n",
    ] {
        assert_eq!(
            fmt(src),
            src,
            "the delimited arm rewrote a round-tripping form"
        );
        round_trips(src);
    }
}

//! The canonical writer spells a line block's `hard_break` as a bare newline,
//! except where the bare newline would be RE-READ as something else
//! (grammar PART 11 §7c; carve#1334).
//!
//! Every one of these failed silently for as long as it did because the render
//! cannot see it: `to_html(fmt(x)) == to_html(x)` held while the space was
//! destroyed, while one stanza came back as two, and while a block's last line
//! lost its trailing break. The tree and the bytes are what separate them.

/// PART 11 §1's two properties, asserted on the TREE rather than the render.
fn round_trips(source: &str) {
    let out = carve::to_carve(source);
    // The TREE, not the whole `Document`: `source_len` counts the bytes that
    // were read, and a writer that changes bytes is the thing under test.
    assert_eq!(
        carve::parse(&out).children,
        carve::parse(source).children,
        "parse(fmt(x)) != parse(x)\n  source: {source:?}\n  fmt:    {out:?}"
    );
    assert_eq!(
        carve::to_carve(&out),
        out,
        "fmt is not idempotent\n  source: {source:?}"
    );
}

/// The reported shape. PART 7 makes the run before a line-break backslash
/// INTERIOR, so a verse line may end in a LONE space; drop the backslash and
/// PART 2's NO TRAILING WHITESPACE clause takes the space with it.
#[test]
fn a_lone_trailing_space_keeps_the_backslash() {
    assert_eq!(
        carve::to_carve("::: |\na \\\nb\n:::\n"),
        "::: |\na \\\nb\n:::\n"
    );
    round_trips("::: |\na \\\nb\n:::\n");
}

/// A run of TWO OR MORE columns is already NBSP content (PART 9 §23 MEDIAL
/// GAPS) and survives a bare newline, so the writer owes it no backslash. The
/// control that keeps the rule from being "always emit one".
#[test]
fn a_two_column_trailing_run_needs_no_backslash() {
    assert_eq!(
        carve::to_carve("::: |\na  \\\nb\n:::\n"),
        "::: |\na  \nb\n:::\n"
    );
    round_trips("::: |\na  \\\nb\n:::\n");
}

/// A `\` ALONE on a body line is how a stanza carries an EMPTY verse line. A
/// bare newline leaves a BLANK line, which ends the stanza - the worse loss of
/// the two, because it returns one stanza as two.
#[test]
fn a_backslash_only_line_keeps_its_backslash() {
    assert_eq!(
        carve::to_carve("::: |\na\n\\\nb\n:::\n"),
        "::: |\na\n\\\nb\n:::\n"
    );
    round_trips("::: |\na\n\\\nb\n:::\n");

    let out = carve::to_carve("::: |\na\n\\\nb\n:::\n");
    assert_eq!(
        carve::to_html(&out).matches("<p>").count(),
        1,
        "the stanza was split: {out:?}"
    );
}

/// A block's LAST body line ending in `\` - the third failure, and the one no
/// corpus `.fmt` covers. The trailing break and the space it holds interior
/// both have to survive.
#[test]
fn a_last_body_line_keeps_its_trailing_break() {
    round_trips("::: |\na\nb \\\n:::\n");

    let out = carve::to_carve("::: |\na\nb \\\n:::\n");
    assert!(
        out.contains("b \\\n"),
        "the trailing break lost its space: {out:?}"
    );
    assert_eq!(
        carve::to_html(&out),
        "<div class=\"line-block\">\n  <p>a<br>\nb <br>\n</p>\n</div>"
    );
}

/// THE BREAK IS NOT ADDITIVE (PART 9 §23): `hard_break` consumes its own
/// newline, so nothing survives for the container to harden and one boundary
/// produces one `<br>`, however it is spelled.
#[test]
fn one_boundary_produces_one_break() {
    assert_eq!(
        carve::to_html("::: |\na \\\nb\n:::\n"),
        "<div class=\"line-block\">\n  <p>a <br>\nb</p>\n</div>"
    );
    assert_eq!(
        carve::to_html("::: |\na\nb\n:::\n"),
        "<div class=\"line-block\">\n  <p>a<br>\nb</p>\n</div>"
    );
}

/// The plain lines the rule must leave alone: no trailing whitespace, nothing
/// empty, so the tree is identical either way and the bytes stay bare.
#[test]
fn an_ordinary_verse_line_still_ends_in_a_bare_newline() {
    assert_eq!(carve::to_carve("::: |\na\nb\n:::\n"), "::: |\na\nb\n:::\n");
    round_trips("::: |\na\nb\nc\n:::\n");
}

/// §7c enumerates two cases, and both are about the line the break ENDS. A
/// break that ends the STANZA has no line after it to be re-read against, so
/// the clause's ground - that the tree is identical either way - does not hold
/// there and the break is simply lost. Neither of these involves whitespace at
/// all, which is why the enumerated cases miss them.
#[test]
fn a_break_ending_the_stanza_keeps_its_backslash_with_no_space_involved() {
    round_trips("::: |\na\\\n:::\n");
    round_trips("::: |\na  \\\n:::\n");

    assert!(carve::to_carve("::: |\na\\\n:::\n").contains("a\\\n"));
    assert_eq!(
        carve::to_html(&carve::to_carve("::: |\na\\\n:::\n")),
        carve::to_html("::: |\na\\\n:::\n")
    );
}

/// A break that ends a stanza in the MIDDLE of a block is the same case: the
/// bare newline becomes the blank line that separates the stanzas, and the
/// break it stood for is gone.
#[test]
fn a_break_ending_a_middle_stanza_keeps_its_backslash() {
    round_trips("::: |\na\\\n\nb\n:::\n");
}

/// A LINE ENDING IN A COMMENT takes no backslash, whatever else is true of it:
/// the marker runs to end of line, so the backslash lands in the comment's
/// CONTENT. An empty comment writes back with a trailing space, which is exactly
/// what the lone-space case looks for - so without the exemption the writer
/// publishes a backslash as comment text, and neither the HTML nor idempotence
/// can see it.
#[test]
fn a_line_ending_in_a_comment_takes_no_backslash() {
    for source in [
        "::: |\na %%\nb\n:::\n",
        "::: |\n%%\nb\n:::\n",
        "::: |\nx %% c\nb\n:::\n",
    ] {
        round_trips(source);
        let out = carve::to_carve(source);
        assert!(
            !out.contains("%% \\"),
            "a backslash landed inside the comment: {out:?}"
        );
    }
}

/// A run that stays open swallows the emptied comment line as a NEWLINE, so its
/// value holds an EMPTY LINE - which the writer cannot emit as a blank one,
/// because a blank line ends the stanza and the run comes back split. A comment
/// line is the one spelling of an empty verse line that survives inside a run,
/// because the block layer takes it before the run exists.
#[test]
fn an_empty_line_inside_a_run_writes_back_as_a_comment_line() {
    let source = "::: |\na `b\n%% secret\nc\n:::\n";
    let out = carve::to_carve(source);

    assert_eq!(out, "::: |\na `b\n%%\nc`\n:::\n");
    assert!(
        !out.contains("secret"),
        "the comment came back from a tree that does not hold it: {out:?}"
    );
    round_trips(source);
}

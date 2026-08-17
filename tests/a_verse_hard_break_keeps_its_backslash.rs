//! A line block's `hard_break` is written BARE where, and only where, re-reading
//! that newline yields the same tree (grammar PART 11 §7c; carve#1334, amended
//! by carve#1340).
//!
//! §7c is a PROPERTY, not a list. A bare newline re-derives a break at a
//! boundary BETWEEN two body lines and nowhere else, because that is the
//! boundary PART 9 §23 hardens; the cases below are consequences of that. The
//! clause was first written as a list of two, and the case it did not reach -
//! the last body line - is pinned here beside them.
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

/// The last body line's backslash carries NO newline of its own: the newline
/// after it belongs to the closing fence. Emitting one leaves a blank line
/// inside the block, which is a stanza boundary the author did not write.
#[test]
fn a_last_line_break_writes_no_newline_of_its_own() {
    assert_eq!(carve::to_carve("::: |\na\\\n:::\n"), "::: |\na\\\n:::\n");
    assert_eq!(
        carve::to_carve("::: |\na  \\\n:::\n"),
        "::: |\na  \\\n:::\n"
    );
}

/// WHICH LINE IS LAST IS DECIDED BY THE BREAKS, however the author spelled
/// them. `a` over a comment and `a \` over the same comment are one document
/// apart by a backslash, and the comment is a node in both - so the last body
/// line here is the COMMENT, and the backslash on the line above is there under
/// the lone-space case rather than under the last-line one.
///
/// ASSERTED ON THE BYTES, because the round-trip gate cannot see this one: a
/// dropped `comment` node renders nothing either way, and a writer that lost it
/// would still satisfy every invariant in `round_trips`.
#[test]
fn a_boundary_the_author_spelled_is_still_a_boundary() {
    assert_eq!(
        carve::to_carve("::: |\na \\\n%% c\n:::\n"),
        "::: |\na \\\n%% c\n:::\n"
    );
    assert_eq!(
        carve::to_carve("::: |\na\n%% c\n:::\n"),
        "::: |\na\n%% c\n:::\n"
    );
}

/// An EMPTY comment is the marker and nothing else. The space after the marker
/// separates it from content; with no content it is line-trailing whitespace,
/// which PART 2 discards and §7 therefore lets the writer drop - and in a line
/// block it is the very space §7c's lone-space case looks for, so emitting it
/// made the writer propose a backslash for a line with nothing to protect.
#[test]
fn an_empty_comment_writes_back_as_the_bare_marker() {
    assert_eq!(
        carve::to_carve("::: |\na\n%%\nb\n:::\n"),
        "::: |\na\n%%\nb\n:::\n"
    );
    assert_eq!(carve::to_carve("a %%\n"), "a %%\n");
}

/// THE EXEMPTION IS KEYED ON THE NODE, NOT ON THE LINE'S POSITION, which is the
/// same clause reaching the last-body-line consequence: a writer that appends
/// the backslash to whatever it emitted last on the final line writes it inside
/// the note, on a line where there was no break to protect.
///
/// Reached through the AST, because no SOURCE produces this tree - the marker
/// runs to the end of its line, so an authored comment never has a break after
/// it. An ingested document can, and PART 11 answers for that document too.
#[test]
fn a_trailing_break_after_a_comment_writes_nothing_into_the_note() {
    let wire = r#"{"type":"document","srcByteLength":0,"children":[
        {"type":"line_block","children":[
            {"type":"paragraph","children":[
                {"type":"text","value":"a"},
                {"type":"hard_break"},
                {"type":"comment","block":false,"content":"c"},
                {"type":"hard_break"}
            ]}
        ]}
    ]}"#;
    let doc = carve::from_json(wire).expect("the wire document decodes");
    let out = carve::render_carve(&doc).expect("a decoded tree renders");

    assert_eq!(out, "::: |\na\n%% c\n:::\n");
    assert!(
        !out.contains("%% c\\"),
        "a backslash landed inside the note: {out:?}"
    );
}

/// A LONE TRAILING COLUMN IS NOT ONLY A SPACE. §7c's list names the plain one;
/// an ESCAPED space is the same consequence of the same property, and it is lost
/// harder - PART 11 §2a writes an escaped space at the END of a line as a bare
/// backslash, and in verse a bare backslash at end of line is a HARD BREAK, so
/// the column does not come back at all.
///
/// Derived from the property rather than read off the list, which is the whole
/// reason carve#1340 made the property normative and the bullets consequences.
#[test]
fn an_escaped_trailing_column_keeps_the_break_that_holds_it_interior() {
    for source in [
        "::: |\na\\ \\\nb\n:::\n",
        "::: |\na\\ \\ \nb\n:::\n",
        "::: |\na\\ \\\n:::\n",
    ] {
        round_trips(source);
    }

    // The escape comes back with its space, because the break's backslash puts
    // it back INSIDE the line where §2a keeps it.
    assert_eq!(
        carve::to_carve("::: |\na\\ \\\nb\n:::\n"),
        "::: |\na\\ \\\nb\n:::\n"
    );
}

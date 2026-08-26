//! THE COLUMN GATE IS ONE OPERATION IN EVERY CONTAINER (PART 9 §17 L3,
//! markup-carve/carve#1814, markup-carve/carve-rs#1446).
//!
//! `AND FLUSH-LEFT MEANS COLUMN 0` (§17 L3, markup-carve/carve#1436) says the
//! `+` marker attaches a block that begins at DOCUMENT column 0 and nothing
//! else. A line at any other column is not attached at all: it falls through to
//! the ordinary column rules, which give it to whichever container its own
//! column names, "exactly as if the `+` line had been a comment".
//!
//! That question was asked in the LIST ITEM and nowhere else, so a footnote
//! body, a definition description and a block quote each reached out for a line
//! the clause leaves where the author wrote it.
//!
//! THE CLAUSE NAMES ITS OWN CONTROL, so the rule is a RELATION between two
//! documents and no single golden can express it: for every container, the
//! marker spelling and the comment spelling of the same document must render
//! the same thing. A change that fixes three containers and drifts the fourth
//! passes every golden it did not touch.
//!
//! The QUOTE row uses the blank-line control as well. A comment line at column
//! 0 under an OPEN quoted paragraph is folded into it as lazy text rather than
//! being skipped - a defect of the quote's invisible-line handling and not of
//! the marker, deliberately left by markup-carve/carve#1817 - so the row closes
//! the quoted paragraph with a bare `>` first and asks about the column alone.

/// Whitespace before a closing tag is dropped as well as collapsed: a comment
/// line inside a list item leaves a trailing space in the item's text where the
/// marker leaves none. That is the comment's own layout artifact and says
/// nothing about which container the line after it reached, which is the only
/// thing these rows ask.
fn html(src: &str) -> String {
    let rendered = carve::to_html(src);
    let collapsed = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.replace("> <", "><").replace(" </", "</").trim().to_string()
}

/// The same document twice, `+` where the control has its invisible line.
fn marker(src: &str) -> String {
    src.replacen('@', "+", 1)
}

fn comment(src: &str) -> String {
    src.replacen('@', "%% c", 1)
}

const BAND: &[(&str, &str)] = &[
    (
        "a footnote body, below its minimum column",
        "[^a]: intro\n@\n more\n\nsee[^a]\n",
    ),
    (
        "a footnote body, at its minimum column",
        "[^a]: intro\n@\n  more\n\nsee[^a]\n",
    ),
    (
        "a description, below its content column",
        ":: term\n:  intro\n@\n  more\n",
    ),
    (
        "a description, one column further below",
        ":: term\n:  intro\n@\n more\n",
    ),
    (
        "a block quote, with the quoted paragraph closed",
        "> intro\n>\n@\n  more\n",
    ),
    (
        "a list item, the container that always held the gate",
        "- intro\n@\n  more\n",
    ),
];

#[test]
fn the_marker_reaches_no_further_than_a_comment_does() {
    for (what, src) in BAND {
        assert_eq!(
            html(&marker(src)),
            html(&comment(src)),
            "the marker reached further than a comment does in {what}"
        );
    }
}

#[test]
fn the_quote_agrees_with_its_blank_line_control_too() {
    let src = "> intro\n>\n@\n  more\n";
    assert_eq!(html(&marker(src)), html(&src.replacen("@\n", "", 1)));
}

/// The positive half. A gate that refused everything would satisfy every
/// assertion above, so each container is asked the SAME document one column
/// over, where the marker does attach.
const ATTACHES: &[(&str, &str, &str)] = &[
    (
        "a footnote body",
        "[^a]: intro\n+\nmore\n\nsee[^a]\n",
        concat!(
            "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>",
            "<section role=\"doc-endnotes\" aria-label=\"Footnotes\"><hr><ol><li id=\"fn1\">",
            "<p>intro</p><p>more<a href=\"#fnref1\" role=\"doc-backlink\" ",
            "aria-label=\"Back to reference\">↩</a></p></li></ol></section>"
        ),
    ),
    (
        "a description",
        ":: term\n:  intro\n+\nmore\n",
        "<dl><dt>term</dt><dd><p>intro</p><p>more</p></dd></dl>",
    ),
    (
        "a block quote",
        "> intro\n>\n+\nmore\n",
        "<blockquote><p>intro</p><p>more</p></blockquote>",
    ),
    (
        "a list item",
        "- intro\n+\nmore\n",
        "<ul><li>intro more</li></ul>",
    ),
];

#[test]
fn a_column_zero_block_still_attaches() {
    for (what, src, expected) in ATTACHES {
        assert_eq!(html(src), *expected, "the marker stopped attaching in {what}");
    }
}

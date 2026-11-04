//! Continuation markers attach at their open container's marker column.
//! A refused `+` remains text in a list, quote, description, or footnote.
//! The quote control checks the ordinary text path after a bare `>` line.

/// Whitespace before a closing tag is dropped as well as collapsed: a comment
/// line inside a list item leaves a trailing space in the item's text where the
/// marker leaves none. That is the comment's own layout artifact and says
/// nothing about which container the line after it reached, which is the only
/// thing these rows ask.
fn html(src: &str) -> String {
    let rendered = carve::to_html(src);
    let collapsed = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .replace("> <", "><")
        .replace(" </", "</")
        .trim()
        .to_string()
}

/// The same document twice, `+` where the control has its invisible line.
fn marker(src: &str) -> String {
    src.replacen('@', "+", 1)
}

#[test]
fn the_quote_keeps_a_refused_plus_as_text() {
    let src = "> intro\n>\n@\n  more\n";
    assert_eq!(
        html(&marker(src)),
        "<blockquote><p>intro</p></blockquote><p>more</p>"
    );
    assert_eq!(
        html(&src.replacen("@\n", "", 1)),
        "<blockquote><p>intro</p></blockquote><p>more</p>"
    );
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
    let drifted: Vec<&str> = ATTACHES
        .iter()
        .filter(|(_, src, expected)| html(src) != **expected)
        .map(|(what, _, _)| *what)
        .collect();
    assert!(
        drifted.is_empty(),
        "the marker stopped attaching in: {}",
        drifted.join("; ")
    );
}

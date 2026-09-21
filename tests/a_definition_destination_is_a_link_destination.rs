//! A reference definition's destination is `link_destination`, the same
//! production as the inline tail's: a parenthesis reaches it only through
//! `balanced_parens` or `destination_escape`, and those three escapes resolve.
//! CARVE-P3-005 anchors the line at its newline, so a run that is not a
//! destination leaves content over and the line is an ordinary paragraph.
//!
//! The trailing attribute block is read AFTER the destination and the title
//! rather than split off the target first. A scan walking the target from its
//! start cannot tell a brace or a quote in the DESTINATION from one opening the
//! block, so `[a]: /u{x} {.c}` and `[a]: it's {.c}` were prose
//! (markup-carve/carve-rs#1791).

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

// Asserted as the WHOLE rendering: "no link" also describes an engine that
// dropped the line, and the fallback the anchor asks for is the author's line
// surviving as text.
#[test]
fn an_unclosed_parenthesis_leaves_the_line_as_prose() {
    assert_eq!(
        html("[a]: a(b\n\n[x][a]\n"),
        "<p>[a]: a(b</p>\n<p>[x][a]</p>"
    );
}

#[test]
fn a_parenthesis_with_no_opener_leaves_the_line_as_prose() {
    assert_eq!(
        html("[a]: a)b\n\n[x][a]\n"),
        "<p>[a]: a)b</p>\n<p>[x][a]</p>"
    );
}

/// The inner pair balances and the outer opener does not, so a counter that
/// only asks "did every `)` find an opener" reads this as a definition. `(c)`
/// becomes the copyright sign once the line is prose.
#[test]
fn an_outer_opener_left_unclosed_leaves_the_line_as_prose() {
    assert_eq!(
        html("[a]: a(b(c)d\n\n[x][a]\n"),
        "<p>[a]: a(b\u{a9}d</p>\n<p>[x][a]</p>"
    );
}

/// The counts match and the order does not.
#[test]
fn a_closer_before_its_opener_leaves_the_line_as_prose() {
    assert_eq!(
        html("[a]: )a(\n\n[x][a]\n"),
        "<p>[a]: )a(</p>\n<p>[x][a]</p>"
    );
}

/// The title slot opens only after the destination, so a run that is not one is
/// not rescued by what follows it.
#[test]
fn an_unclosed_parenthesis_before_a_title_leaves_the_line_as_prose() {
    assert_eq!(
        html("[a]: a(b \"T\"\n\n[x][a]\n"),
        "<p>[a]: a(b \u{201c}T\u{201d}</p>\n<p>[x][a]</p>"
    );
}

#[test]
fn a_definition_carries_the_resolved_destination() {
    for (src, href) in [
        ("[a]: a(b)c\n\n[x][a]\n", "a(b)c"),
        ("[a]: a((b))c\n\n[x][a]\n", "a((b))c"),
        ("[a]: a\\(b\n\n[x][a]\n", "a(b"),
        ("[a]: a\\)b\n\n[x][a]\n", "a)b"),
        ("[a]: a\\\\b\n\n[x][a]\n", "a\\b"),
        // A backslash before anything else is an ordinary destination
        // character, so URLs full of backslashes need no doubling.
        ("[a]: a\\b\n\n[x][a]\n", "a\\b"),
    ] {
        assert_eq!(
            html(src),
            format!("<p><a href=\"{href}\">x</a></p>"),
            "{src:?}"
        );
    }
}

/// The inline tail has always read the production, and it is the control: the
/// same run answers the same way on both sides of the language.
#[test]
fn the_inline_tail_reads_the_same_run() {
    assert_eq!(html("[t](a(b)\n"), "<p>[t](a(b)</p>");
    assert_eq!(html("[t](a\\(b)\n"), "<p><a href=\"a(b\">t</a></p>");
    assert_eq!(html("[t](a\\)b)\n"), "<p><a href=\"a)b\">t</a></p>");
}

/// An image reference resolves the same entry (CARVE-P3-008), so it takes the
/// resolved destination too.
#[test]
fn an_image_reference_takes_the_resolved_destination() {
    assert_eq!(
        html("![alt][a]\n\n[a]: /i\\(x.png\n"),
        "<img src=\"/i(x.png\" alt=\"alt\">"
    );
}

/// The writer re-escapes what the reader resolved. Writing the resolved value
/// bare would emit `[a]: a(b`, and one `fmt` pass would lose the definition and
/// every link resolving it.
#[test]
fn the_writer_re_escapes_the_destination() {
    for src in [
        "[x][a]\n\n[a]: a\\(b\n",
        "[x][a]\n\n[a]: a\\)b\n",
        "[x][a]\n\n[a]: a(b)c\n",
        "[x][a]\n\n[a]: a\\(b \"T\" {.c}\n",
    ] {
        assert_eq!(fmt(src), src, "{src:?}");
        assert_eq!(html(&fmt(src)), html(src), "{src:?}");
    }
}

/// A brace or a quote in the destination no longer hides the trailing block.
#[test]
fn a_brace_or_a_quote_does_not_hide_the_trailing_block() {
    assert_eq!(
        html("[a]: /u{x} {.c}\n\n[x][a]\n"),
        "<p><a href=\"/u{x}\" class=\"c\">x</a></p>"
    );
    assert_eq!(
        html("[a]: it's {.c}\n\n[x][a]\n"),
        "<p><a href=\"it&apos;s\" class=\"c\">x</a></p>"
    );
}

/// The shapes around it, which the change must not move. With no space the
/// braces are destination; a run leaves the block unconsumed; a block
/// `attributes` does not accept is leftover content (CARVE-P3-006); and a `}`
/// inside a quoted value does not end the block.
#[test]
fn the_neighbouring_block_shapes_are_unchanged() {
    assert_eq!(
        html("[a]: /u{.c}\n\n[x][a]\n"),
        "<p><a href=\"/u{.c}\">x</a></p>"
    );
    assert_eq!(
        html("[a]: /u  {.c}\n\n[x][a]\n"),
        "<p>[a]: /u  {.c}</p>\n<p>[x][a]</p>"
    );
    assert_eq!(
        html("[a]: /u {#}\n\n[x][a]\n"),
        "<p>[a]: /u {#}</p>\n<p>[x][a]</p>"
    );
    assert_eq!(
        html("[a]: /u {k=\"a} b\"}\n\n[x][a]\n"),
        "<p><a href=\"/u\" k=\"a} b\">x</a></p>"
    );
    assert_eq!(html("[a]: {x}\n\n[x][a]\n"), "<p><a href=\"{x}\">x</a></p>");
}

/// The title slot is unchanged by the reorder: both quote spellings, the escape
/// inside the run, and the cardinality PART 7 holds it to.
#[test]
fn the_title_slot_is_unchanged() {
    assert_eq!(
        html("[a]: /u \"T\"\n\n[x][a]\n"),
        "<p><a href=\"/u\" title=\"T\">x</a></p>"
    );
    assert_eq!(
        html("[a]: /u 'T'\n\n[x][a]\n"),
        "<p><a href=\"/u\" title=\"T\">x</a></p>"
    );
    assert_eq!(
        html("[a]: /u \"a\\\"b\"\n\n[x][a]\n"),
        "<p><a href=\"/u\" title=\"a&quot;b\">x</a></p>"
    );
    assert_eq!(
        html("[a]: /u  \"T\"\n\n[x][a]\n"),
        "<p>[a]: /u  \u{201c}T\u{201d}</p>\n<p>[x][a]</p>"
    );
    assert_eq!(
        html("[a]: /u \"T\n\n[x][a]\n"),
        "<p>[a]: /u \u{201c}T</p>\n<p>[x][a]</p>"
    );
}

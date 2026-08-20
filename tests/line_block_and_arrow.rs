//! The two gaps from the implementation audit (carve#130): a `=>` run no longer
//! opens a `=` highlight span, and `::: |` line blocks are implemented.

#[test]
fn arrow_does_not_open_a_highlight() {
    // `=>` stopped being an arrow in markup-carve/carve#1442, which makes this
    // case MORE load-bearing rather than less: the `=` is now exposed to the
    // emphasis machinery, and without the guard it opens a highlight here and
    // closes on the `=` of `!=` - carve-js rendered `<mark>&gt; b; x !</mark>`
    // before the same guard was added there.
    assert_eq!(carve::to_html("a => b; x != y\n"), "<p>a =&gt; b; x ≠ y</p>");
    assert_eq!(carve::to_html("a ==> b; x != y\n"), "<p>a ⇒ b; x ≠ y</p>");
    // a real highlight still works
    assert_eq!(carve::to_html("a =hi= b\n"), "<p>a <mark>hi</mark> b</p>");
}

#[test]
fn line_block_renders_as_a_verse_div() {
    let html = carve::to_html("::: |\nRoses are red,\n  Violets are blue.\n\nStanza two.\n:::\n");
    assert_eq!(
        html,
        "<div class=\"line-block\">\n  <p>Roses are red,<br>\n&nbsp;&nbsp;Violets are blue.</p>\n  <p>Stanza two.</p>\n</div>"
    );
}

// Strict column-0 rule (docs/divergence-from-djot.md §11): a colon-fence family
// opener recognized only at its container's content column. An INDENTED `::: |`
// line block or `::: \` hard-break block (above the top-level content column,
// column 0) must NOT open; the whole run folds to literal paragraph text.
// Matches carve-js.

#[test]
fn indented_line_block_stays_literal() {
    // One space of leading indent: the `::: |` opener does not fire.
    assert_eq!(
        carve::to_html(" ::: |\n Roses are red,\n Violets are blue.\n :::\n"),
        "<p>::: |\nRoses are red,\nViolets are blue.\n:::</p>"
    );
}

#[test]
fn indented_hardbreak_block_stays_literal() {
    // One space of leading indent: the `::: \` opener does not fire.
    assert_eq!(
        carve::to_html(" ::: \\\n one\n two\n :::\n"),
        "<p>::: <br>\none\ntwo\n:::</p>"
    );
}

#[test]
fn flush_left_line_block_still_opens() {
    // Regression anchor: a column-0 line block MUST keep opening -- the
    // column-0 guard only suppresses INDENTED openers.
    assert_eq!(
        carve::to_html("::: |\nRoses are red,\nViolets are blue.\n:::\n"),
        "<div class=\"line-block\">\n  <p>Roses are red,<br>\nViolets are blue.</p>\n</div>"
    );
}

#[test]
fn flush_left_hardbreak_block_still_opens() {
    // Regression anchor: a column-0 hard-break block MUST keep opening.
    assert_eq!(
        carve::to_html("::: \\\none\ntwo\n:::\n"),
        "<div class=\"hardbreaks\">\n  <p>one<br>\ntwo</p>\n</div>"
    );
}

#[test]
fn indented_line_block_does_not_interrupt_a_paragraph() {
    // An indented `::: |` following paragraph text folds into the paragraph
    // rather than splitting it -- the interruption path obeys the same
    // column-0 guard as the opener path.
    assert_eq!(
        carve::to_html("a\n ::: |\n b\n :::\n"),
        "<p>a\n::: |\nb\n:::</p>"
    );
}

#[test]
fn indented_colon_fence_in_quote_keeps_lazy_continuation() {
    // Inside a block quote the content column is column 0 of the stripped body.
    // A colon fence indented above it (`>  ::: |`, two spaces after `>`) is
    // literal, so the unquoted `lazy` line stays in the quote. Matches carve-js
    // and the executable-spec oracle.
    assert_eq!(
        carve::to_html("> a\n>  ::: |\nlazy\n"),
        "<blockquote><p>a\n::: |\nlazy</p></blockquote>"
    );
    assert_eq!(
        carve::to_html("> a\n>  ::: note\nlazy\n"),
        "<blockquote><p>a\n::: note\nlazy</p></blockquote>"
    );
}

#[test]
fn flush_colon_fence_in_quote_ends_lazy_continuation() {
    // Regression anchor: a colon fence at the quote's content column (`> ::: |`,
    // one space after `>`) IS a real opener, so it ends lazy continuation and
    // `lazy` detaches to the document level. Matches the oracle.
    assert_eq!(
        carve::to_html("> a\n> ::: |\nlazy\n"),
        concat!(
            "<blockquote>\n",
            "  <p>a</p>\n",
            "  <div class=\"line-block\">\n",
            "  </div>\n",
            "</blockquote>\n",
            "<p>lazy</p>"
        )
    );
    assert_eq!(
        carve::to_html("> a\n> ::: note\nlazy\n"),
        concat!(
            "<blockquote>\n",
            "  <p>a</p>\n",
            "  <aside class=\"admonition note\">\n",
            "\n",
            "  </aside>\n",
            "</blockquote>\n",
            "<p>lazy</p>"
        )
    );
}

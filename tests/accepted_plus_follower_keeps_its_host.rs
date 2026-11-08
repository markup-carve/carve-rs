use carve::to_html;

#[test]
fn a_later_marker_line_note_keeps_its_structural_follower() {
    for indent in 2..=4 {
        let input = format!(
            "[^a]: +\n[^b]: +\n{}> quote\n\nsee[^a][^b]\n",
            " ".repeat(indent)
        );
        let html = to_html(&input);
        let first = html.split("<li id=\"fn1\">").nth(1).expect("first note");
        let second = html.split("<li id=\"fn2\">").nth(1).expect("second note");
        assert!(
            !first.split("</li>").next().unwrap().contains("quote"),
            "{input}: {html}"
        );
        assert!(
            second
                .split("</li>")
                .next()
                .unwrap()
                .contains("<blockquote><p>quote</p></blockquote>"),
            "{input}: {html}"
        );
    }
}

#[test]
fn a_losing_marker_line_preserves_its_surviving_notes_list_boundary() {
    let input = "[^n]: +\n[^n]: +\n  - item\n- sibling\n\nsee[^n]\n";
    let html = to_html(input);
    assert!(
        html.starts_with("<ul>\n  <li>sibling</li>\n</ul>\n"),
        "{html}"
    );
    assert!(
        html.contains("<li id=\"fn1\">\n      <ul>\n        <li>item</li>\n      </ul>"),
        "{html}"
    );
}

#[test]
fn an_indented_quote_prefix_does_not_move_the_follower_out_of_its_item() {
    let source = "> - - - +\n  >   x\n";
    let html = to_html(source);
    assert!(html.contains("<li>x</li>"), "{html}");
    assert!(!html.contains("<p>x</p>"), "{html}");
}

#[test]
fn the_shallower_follower_keeps_the_same_owner() {
    let source = "> - - +\n  > x\n";
    let html = to_html(source);
    assert!(html.contains("<li>x</li>"), "{html}");
}

#[test]
fn a_losing_marker_line_plus_does_not_own_the_band_follower() {
    let input = "[^a]: +\n[^a]: +\n flush\n\nsee[^a]\n";
    let html = to_html(input);
    assert!(html.starts_with("<p>flush</p>\n<p>see"), "{html}");
    assert!(html.contains("<li id=\"fn1\">"), "{html}");
    assert!(!html.contains("<p>flush<a href=\"#fnref1\""), "{html}");
}

#[test]
fn repeated_marker_lines_keep_the_follower_outside() {
    for count in 2..=4 {
        let input = format!("{} flush\n\nsee[^a]\n", "[^a]: +\n".repeat(count));
        let html = to_html(&input);
        assert!(html.starts_with("<p>flush</p>\n<p>see"), "{input}: {html}");
    }
}

#[test]
fn a_winning_note_plus_before_a_repeated_other_label_stays_consumed() {
    let input = "[^a]: +\n[^b]: +\n[^b]: +\n\nsee[^b]\n";
    let html = to_html(input);
    assert!(html.contains("<li id=\"fn1\">"), "{html}");
    assert!(!html.contains("<p>+"), "{html}");
}

// carve#5474 (ruled A): a losing duplicate's `+` takes nothing.
#[test]
fn a_losing_plus_after_a_plain_first_definition_takes_nothing() {
    for (input, flush) in [
        (
            "[^a]: x\n[^a]: +\n flush\n\nsee[^a]\n",
            "<p>flush</p>\n<p>see",
        ),
        (
            "[^a]: x\n[^a]: +\nflush\n\nsee[^a]\n",
            "<p>flush</p>\n<p>see",
        ),
        (
            "[^a]: x\n[^a]: +\n[^a]: +\nflush\n\nsee[^a]\n",
            "<p>flush</p>\n<p>see",
        ),
        (
            "[^a]: +\n x\n[^a]: +\nflush\n\nsee[^a]\n",
            "<p>flush</p>\n<p>see",
        ),
        (
            "> [^a]: x\n> [^a]: +\n> flush\n\nsee[^a]\n",
            "<blockquote><p>flush</p></blockquote>",
        ),
    ] {
        let html = to_html(input);
        assert!(html.starts_with(flush), "{input}: {html}");
        assert!(html.contains("<p>x<a href=\"#fnref1\""), "{input}: {html}");
    }
}

#[test]
fn a_winning_plus_takes_the_block_past_a_losing_plus() {
    for input in [
        "see[^a]\n\n[^a]: +\n[^a]: +\nflush\n",
        "see[^a]\n\n[^a]: +\n[^a]: x\n[^a]: +\nflush\n",
        "- see[^a]\n\n  [^a]: +\n  [^a]: +\n  flush\n",
    ] {
        let html = to_html(input);
        assert!(
            html.contains("<p>flush<a href=\"#fnref1\""),
            "{input}: {html}"
        );
    }
}

// carve#5474 and PART 9R R2: a losing duplicate registers nothing and its `+`
// takes nothing; a winning `+` still takes the next block.

#[test]
fn a_losing_plus_of_another_label_leaves_the_block_to_the_winner() {
    assert_eq!(
        to_html("see[^a][^b]\n\n[^a]: x\n[^b]: +\n[^a]: +\nflush\n").trim_end(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a><a id=\"fnref2\" href=\"#fn2\" role=\"doc-noteref\"><sup>2</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n    <li id=\"fn2\">\n      <p>flush<a href=\"#fnref2\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n".trim_end()
    );
}

#[test]
fn a_skipped_plus_definition_is_no_literal_plus() {
    assert_eq!(
        to_html("see[^b]\n\n[^a]: +\n[^b]: +\n[^b]: +\nflush\n").trim_end(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n".trim_end()
    );
}

#[test]
fn three_definitions_in_an_item_give_the_block_to_the_winner() {
    assert_eq!(
        to_html("- see[^a]\n  [^a]: +\n  [^a]: +\n  [^a]: +\n  flush\n").trim_end(),
        "<ul>\n  <li>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>flush<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n".trim_end()
    );
}

#[test]
fn a_winning_plus_takes_the_block_past_a_text_and_a_losing_plus() {
    assert_eq!(
        to_html("[^n]: +\n[^n]: d1\n[^n]: +\n  - item\n\nsee[^n]\n").trim_end(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>item</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n".trim_end()
    );
}

#[test]
fn the_block_below_a_losing_plus_is_read_in_the_document() {
    assert_eq!(
        to_html("see[^a]\n[^a]: d1\n[^a]: +\n  - item\n").trim_end(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<ul>\n  <li>item</li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>d1<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n".trim_end()
    );
}

#[test]
fn the_block_below_a_losing_plus_is_read_in_the_quote() {
    assert_eq!(
        to_html("> see[^a]\n> [^a]: d1\n> [^a]: +\n>   - item\n").trim_end(),
        "<blockquote>\n  <p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n  <ul>\n    <li>item</li>\n  </ul>\n</blockquote>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>d1<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n".trim_end()
    );
}

#[test]
fn the_block_below_a_losing_plus_is_read_in_the_item() {
    assert_eq!(
        to_html("- see[^a]\n  [^a]: d1\n  [^a]: +\n    - item\n").trim_end(),
        "<ul>\n  <li>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>\n    <ul>\n      <li>item</li>\n    </ul>\n  </li>\n</ul>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>d1<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n".trim_end()
    );
}

#[test]
fn a_winning_plus_takes_the_indented_block_below_its_duplicate() {
    assert_eq!(
        to_html("see[^a]\n[^a]: +\n[^a]: +\n  - item\n").trim_end(),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>item</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>\n".trim_end()
    );
}

// carve#5482 shape 2: a skipped definition still defines its label, so the
// later `[^m]: +` loses and the list goes to the winning `[^n]` (Expected there).
#[test]
fn a_losing_plus_of_a_skipped_label_leaves_the_block_to_the_winner() {
    let html = to_html("[^n]: +\n[^m]: d1\n[^m]: +\n  - item\n\nsee[^n][^m]\n");
    assert!(
        html.contains("<li id=\"fn1\">\n      <ul>\n        <li>item</li>\n      </ul>"),
        "{html}"
    );
    assert!(html.contains("<p>d1<a href=\"#fnref2\""), "{html}");
}

// A definition-shaped line inside a skipped comment span defines nothing.
#[test]
fn a_definition_in_a_skipped_comment_span_defines_no_label() {
    let html = to_html("- +\n%%%\n[^a]: +\n%%%\n[^a]: +\n  - item\n\nsee[^a]\n");
    assert!(
        html.contains("<li id=\"fn1\">\n      <ul>\n        <li>item</li>"),
        "{html}"
    );
}

// A refused note `+` takes the next line lazily, but a definition there
// starts its own note and keeps its body.
#[test]
fn a_definition_after_a_refused_note_plus_keeps_its_body() {
    let html = to_html("see[^a][^b]\n[^b]: +\n[^a]: d2\n  - item\n");
    assert!(
        html.contains("<li id=\"fn1\">\n      <p>d2</p>\n      <ul>\n        <li>item</li>"),
        "{html}"
    );
    let html = to_html("see[^a]\n[^a]: +\n[^a]: d2\n  more\n");
    assert!(!html.contains("more"), "{html}");
}

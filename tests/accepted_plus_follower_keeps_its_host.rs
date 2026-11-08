use carve::to_html;

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

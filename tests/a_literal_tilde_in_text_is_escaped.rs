//! GFM strikethrough pairs a run of one or two tildes, so a literal tilde in a
//! text node is a Markdown metacharacter and PART 11 §8 M1 escapes it. Sibling
//! of markup-carve/carve-js#1718 and markup-carve/carve-php#1983.

#[test]
fn a_text_pair_no_longer_closes_over_the_markup_between_it() {
    assert_eq!(
        carve::to_markdown("a {_~~x_} b {_y~~_} c\n"),
        "a <u>\\~\\~x</u> b <u>y\\~\\~</u> c\n"
    );
}

#[test]
fn a_text_pair_at_the_start_of_a_line_is_escaped() {
    assert_eq!(carve::to_markdown("~~{/x~~/}\n"), "\\~\\~*x\\~\\~*\n");
}

#[test]
fn a_single_tilde_is_escaped_too_because_the_one_tilde_form_pairs() {
    assert_eq!(carve::to_markdown("a ~ b ~ c\n"), "a \\~ b \\~ c\n");
}

#[test]
fn a_path_tilde_is_escaped_which_is_the_cost_of_the_rule() {
    assert_eq!(carve::to_markdown("~/home/user\n"), "\\~/home/user\n");
}

#[test]
fn a_tilde_in_a_table_cell_is_text_like_any_other() {
    assert_eq!(carve::to_markdown("| a~~b |\n"), "| a\\~\\~b |\n");
}

#[test]
fn a_code_span_keeps_its_tilde() {
    assert_eq!(carve::to_markdown("a `x~~y` b\n"), "a `x~~y` b\n");
}

#[test]
fn a_link_destination_keeps_its_tilde() {
    assert_eq!(
        carve::to_markdown("[x](http://a/~b)\n"),
        "[x](http://a/~b)\n"
    );
}

#[test]
fn the_writers_own_strike_delimiters_are_not_escaped() {
    assert_eq!(carve::to_markdown("{~x~}\n"), "~~x~~\n");
}

//! PART 11 §8a M1b's second condition for `_` [CARVE-P11-023]: a literal
//! underscore is escaped when another live one in the same paragraph, heading
//! or table cell could close the emphasis it could open, by CommonMark 6.2
//! read for the underscore.

fn md(src: &str) -> String {
    carve::to_markdown(src)
}

#[test]
fn a_pair_that_would_read_as_emphasis_is_escaped() {
    assert_eq!(md("/x/_y_\n"), "*x*\\_y\\_\n");
}

#[test]
fn only_the_pair_is_escaped_on_a_line_of_bare_underscores() {
    assert_eq!(
        md("/x/_y_ in company_id and a_b_c and a _ b _ c\n"),
        "*x*\\_y\\_ in company_id and a_b_c and a _ b _ c\n"
    );
}

#[test]
fn each_line_answers_for_itself() {
    assert_eq!(md("/x/_y_\n/z/_w_\n"), "*x*\\_y\\_\n*z*\\_w\\_\n");
}

#[test]
fn a_pair_split_across_a_soft_break_is_escaped() {
    // Corpus 468: a reader pairs emphasis across a line break, so the block is
    // the unit rather than the line.
    assert_eq!(md("/x/_y\nz_ w\n"), "*x*\\_y\nz\\_ w\n");
}

#[test]
fn a_pair_split_across_a_blank_line_stays_bare() {
    // Two paragraphs, and the condition is if and only if.
    assert_eq!(md("a/_y\n\nz_ w\n"), "a/_y\n\nz_ w\n");
}

#[test]
fn a_pair_split_across_two_list_items_stays_bare() {
    assert_eq!(md("- a/_y\n- z_ w\n"), "- a/_y\n- z_ w\n");
}

#[test]
fn a_pair_split_across_two_table_cells_stays_bare() {
    assert_eq!(
        md("| a/_y | z_ w |\n|---|---|\n| c | d |\n"),
        "| a/_y | z_ w |\n| --- | --- |\n| c | d |\n"
    );
}

#[test]
fn a_pair_inside_one_table_cell_is_escaped() {
    assert_eq!(
        md("| a/_y z_ w | c |\n|---|---|\n| d | e |\n"),
        "| a/\\_y z\\_ w | c |\n| --- | --- |\n| d | e |\n"
    );
}

#[test]
fn a_pair_split_across_a_quote_s_own_blank_line_stays_bare() {
    assert_eq!(md("> a/_y\n>\n> z_ w\n"), "> a/_y\n>\n> z_ w\n");
}

/// A control: an authored escape is written by M2 and never becomes a
/// candidate, so no mutation of the pairing scan can redden this.
#[test]
fn an_escape_the_author_wrote_supplies_no_half_of_a_pair() {
    assert_eq!(md("/x/\\_y_\n"), "*x*\\_y_\n");
}

#[test]
fn a_snake_case_identifier_stays_bare() {
    assert_eq!(md("company_id\n"), "company_id\n");
    assert_eq!(md("snake_case_name\n"), "snake_case_name\n");
}

#[test]
fn underscores_flanked_on_both_sides_stay_bare() {
    assert_eq!(md("a_b_c\n"), "a_b_c\n");
}

#[test]
fn a_lone_edge_underscore_stays_bare() {
    assert_eq!(md("trailing_\n"), "trailing_\n");
    assert_eq!(md("_leading\n"), "_leading\n");
}

#[test]
fn spaced_underscores_stay_bare() {
    assert_eq!(md("a _ b _ c\n"), "a _ b _ c\n");
}

#[test]
fn a_closer_before_an_opener_stays_bare() {
    assert_eq!(md("x_ _y\n"), "x_ _y\n");
}

#[test]
fn an_intraword_opener_stays_bare() {
    assert_eq!(md("foo_bar_ baz\n"), "foo_bar_ baz\n");
}

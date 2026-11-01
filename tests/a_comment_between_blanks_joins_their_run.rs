//! PART 0 S1 erases a comment from layout, so blanks on both sides of a comment
//! outside every item form one run, and a run of three ends the list
//! (PART 9 §11 N1a, CARVE-P9-014, carve#4728).

fn lists(source: &str) -> usize {
    carve::to_html(source).matches("<ul>").count()
}

#[test]
fn blanks_around_an_erased_comment_count_as_one_run() {
    assert_eq!(lists("- a\n\n\n%% c\n\n- b\n"), 2);
    assert_eq!(lists("- a\n\n%% c\n\n\n- b\n"), 2);
    assert_eq!(lists("- a\n\n%%%\nx\n%%%\n\n\n- b\n"), 2);
}

#[test]
fn a_shorter_run_or_a_comment_inside_the_item_keeps_one_list() {
    assert_eq!(lists("- a\n%% c\n\n\n- b\n"), 1);
    assert_eq!(lists("- a\n\n  %% c\n\n\n- b\n"), 1);
}

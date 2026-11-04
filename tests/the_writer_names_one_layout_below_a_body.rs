//! CARVE-P11-079 (markup-carve/carve#4850) and carve-rs#3757.

fn assert_written(source: &str, expected: &str) {
    let written = carve::to_carve(source);
    assert_eq!(written, expected, "{source}");
    assert_eq!(carve::to_carve(&written), written, "{source}");
    assert_eq!(carve::to_html(&written), carve::to_html(source), "{source}");
}

#[test]
fn a_block_after_a_footnote_list_takes_that_list_column() {
    assert_written(
        "See[^1].\n\n[^1]: - a\n\n\n\n  - b\n",
        "See[^1].\n\n[^1]: - a\n\n\n\n      - b\n",
    );
    assert_written(
        "[r][] [^p] [^c]\n[^c]: - para\n   # H\n",
        "[r][] [^p] [^c]\n\n[^c]: - para\n\n      # H\n",
    );
    assert_written(
        "[^long]: - a\n\n  para\n\n[^long]\n",
        "[^long]\n\n[^long]: - a\n\n         para\n",
    );
}

#[test]
fn a_footnote_body_with_no_list_keeps_two_columns() {
    assert_written("[^n]: a\n\n  para\n\n[^n]\n", "[^n]\n\n[^n]: a\n\n  para\n");
}

#[test]
fn a_literal_plus_after_a_nested_task_fence_is_escaped_at_the_item_column() {
    assert_written(
        "- - [ ] ```js\n        x\n\n  +\ntail\n",
        "- - [ ] ```js\n        x\n\n    ```\n\ntail\n",
    );
}

#[test]
fn a_comment_after_an_attached_paragraph_stays_at_the_content_column() {
    assert_written(
        ". a\n  . %%%\n    n\n  zz\n%%%\n",
        ". a\n  . %%%\n    n\n    %%%\n  zz\n  %%%\n  %%%\n",
    );
}

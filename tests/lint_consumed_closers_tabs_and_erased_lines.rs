use carve::lint_carve;

fn findings(source: &str) -> Vec<(usize, usize, String)> {
    let mut found: Vec<_> = lint_carve(source)
        .into_iter()
        .map(|warning| (warning.line, warning.column, warning.rule.to_string()))
        .collect();
    found.sort();
    found
}

fn row(line: usize, column: usize, rule: &str) -> (usize, usize, String) {
    (line, column, rule.to_string())
}

#[test]
fn a_closer_inside_unclosed_child_code_closes_nothing() {
    assert_eq!(
        findings("::: note\n```\nx\n\n:::\n"),
        vec![
            row(1, 1, "unclosed-container-fence"),
            row(2, 1, "unclosed-code-fence")
        ]
    );
}

#[test]
fn a_closer_inside_an_unclosed_verse_comment_closes_nothing() {
    assert_eq!(
        findings("::: |\na\n%%%\nc\n:::\n"),
        vec![
            row(1, 1, "unclosed-container-fence"),
            row(3, 1, "unclosed-comment-fence")
        ]
    );
}

#[test]
fn a_closer_that_ends_the_item_still_closes_its_container() {
    assert_eq!(
        findings("::: note\n- i\n  ```\n  x\n\n:::\n"),
        vec![row(3, 3, "unclosed-code-fence")]
    );
}

#[test]
fn a_plus_behind_a_lead_tab_is_text() {
    for source in [
        "- - \t+\nx\n",
        " - \t+\n>\n",
        "> - \t- +\nx\n",
        "- \t> +\nx\n",
    ] {
        assert_eq!(findings(source), vec![], "{source:?}");
    }
}

#[test]
fn an_overindented_opener_after_an_erased_comment_reports_its_own_line() {
    assert_eq!(
        findings("> %% c\n>    # h\n"),
        vec![row(2, 6, "block-opener-overindented")]
    );
    assert_eq!(
        findings("> > - ::: box\n> >   - inner\n> >     %% c\n> >    ```\n> >    code\n- tail\n"),
        vec![
            row(1, 7, "unclosed-container-fence"),
            row(4, 8, "list-item-block-overindented"),
            row(4, 8, "unclosed-code-fence"),
        ]
    );
}

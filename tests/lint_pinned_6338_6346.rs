use carve::lint::{LintOptions, lint};

fn ranges(input: &str, code: &str) -> Vec<(u32, u32)> {
    lint(input, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .map(|finding| {
            let pos = finding.pos.expect("diagnostic range");
            (pos.start_offset, pos.end_offset)
        })
        .collect()
}

#[test]
fn alert_tag_has_exact_range_and_glued_near_miss_is_text() {
    assert_eq!(
        ranges(">[!NOTE] > [!TIP]\n", "github-alert-marker"),
        [(1, 8)]
    );
    assert!(ranges(">[!x]: > [!NOTE]\n", "github-alert-marker").is_empty());
}

#[test]
fn braced_inline_at_start_of_nested_leaf_is_not_a_failed_attribute() {
    for input in ["*a\n{_x_}*\n", "::: |\n{_x_} y\n:::\n"] {
        assert!(
            ranges(input, "block-marker-as-text").is_empty(),
            "{input:?}"
        );
    }
}

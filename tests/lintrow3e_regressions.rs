use carve::lint::{LintOptions, lint};

fn ranges(source: &str, code: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|f| f.code == code)
        .filter_map(|f| f.pos.map(|p| (p.start_offset, p.end_offset)))
        .collect()
}

#[test]
fn glued_inline_attributes_are_one_run() {
    assert_eq!(
        ranges("a {.a}{.b}\n", "unattached-inline-attribute"),
        vec![(2, 10)]
    );
    assert_eq!(
        ranges("`c` {.a}{.b}\n", "unattached-inline-attribute"),
        vec![(4, 12)]
    );
}

#[test]
fn parsed_braces_do_not_report_failed_attribute_lines() {
    for source in ["*a\n{_x_}*\n", "::: |\n{_x_} y\n:::\n"] {
        assert!(
            ranges(source, "block-marker-as-text").is_empty(),
            "{source:?}"
        );
    }
}

#[test]
fn only_real_glued_alert_tags_report() {
    assert!(ranges(">[!x]: > [!NOTE]\n", "github-alert-marker").is_empty());
    assert_eq!(ranges(">[!NOTE]\n", "github-alert-marker"), vec![(1, 8)]);
}

#[test]
fn tab_after_quote_mark_is_text() {
    assert_eq!(
        ranges(">\tx\n", "blockquote-marker-without-space"),
        vec![(0, 3)]
    );
}

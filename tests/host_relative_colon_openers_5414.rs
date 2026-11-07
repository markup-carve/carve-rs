use carve::{to_html, to_html_with_options, Options};

fn assert_host_opener(source: &str) {
    let html = to_html(source);
    assert!(html.contains("<div class=\"line-block\">"), "{html}");
    assert!(html.contains("<div>\n\n"), "{html}");
    assert!(!html.contains("<p>::::</p>"), "{html}");
    assert_eq!(
        html,
        to_html_with_options(source, &Options::default().with_positions(true))
    );
}

#[test]
fn a_footnote_opens_the_postblank_colon_run_one_column_past_its_margin() {
    assert_host_opener("[^1]: +\n::: |\n\n   ::::   \n\n[^1]\n");
    assert_host_opener("[^1]: +\n::: |\n\n    ::::   \n\n[^1]\n");
}

#[test]
fn a_description_opens_the_postblank_colon_run_one_column_past_its_margin() {
    assert_host_opener(":: t\n: +\n::: |\n\n   ::::   \n");
    assert_host_opener(":: t\n: +\n::: |\n\n    ::::   \n");
}

#[test]
fn a_colon_opener_at_a_descendant_item_column_stays_in_that_item() {
    let source = ":: t\n: - item\n\n    ::: note\n    body\n    :::\n";
    let html = to_html(source);
    assert!(
        html.contains("<li><p>item</p>\n        <aside class=\"admonition note\""),
        "{html}"
    );
    assert!(!html.contains("<p>body\n:::</p>"), "{html}");
}

#[test]
fn an_existing_marker_line_item_colon_closer_is_not_rebased() {
    let description = to_html(":: t\n: - :::\n\n    :::\n\nq\n");
    assert_eq!(description.matches("<div>").count(), 1, "{description}");
    assert!(description.contains("<p>q</p>"), "{description}");

    let footnote = to_html("[^f]\n\n[^f]: - :::\n\n        :::\n");
    assert_eq!(footnote.matches("<div>").count(), 1, "{footnote}");
}

#[test]
fn a_footnote_marker_line_item_keeps_its_descendant_colon_block() {
    let source = "[^f]\n\n[^f]: - item\n\n        ::: note\n        body\n        :::\n";
    let html = to_html(source);
    assert!(
        html.contains("<li><p>item</p>\n          <aside class=\"admonition note\""),
        "{html}"
    );
    assert!(!html.contains("<p>body\n:::</p>"), "{html}");
}

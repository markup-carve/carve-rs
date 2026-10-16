use carve::{to_html, to_html_with_options, Options};

fn assert_folds(source: &str, expected: &str) {
    let html = to_html(source);
    assert_eq!(
        html,
        to_html_with_options(source, &Options::default().with_positions(true))
    );
    let compact = html
        .lines()
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n")
        .replace(">\n<", "><");
    assert_eq!(compact, expected, "{source:?}");
}

#[test]
fn heading_below_description_column_folds() {
    assert_folds(
        ":: t\n: para\n # H\n",
        "<dl><dt>t</dt><dd>para\n# H</dd></dl>",
    );
    assert_folds(
        ":: t\n:  para\n  # H\n",
        "<dl><dt>t</dt><dd>para\n# H</dd></dl>",
    );
}

#[test]
fn quote_marker_below_description_column_folds() {
    assert_folds(
        ":: t\n: para\n > q\n",
        "<dl><dt>t</dt><dd>para\n&gt; q</dd></dl>",
    );
}

#[test]
fn table_row_below_description_column_folds() {
    assert_folds(
        ":: t\n: para\n | a |\n",
        "<dl><dt>t</dt><dd>para\n| a |</dd></dl>",
    );
}

#[test]
fn quote_marker_at_document_column_ends_description() {
    assert_folds(
        ":: t\n: para\n> q\n",
        "<dl><dt>t</dt><dd>para</dd></dl><blockquote><p>q</p></blockquote>",
    );
}

#[test]
fn below_column_opener_after_a_closed_block_stays_outside() {
    assert_folds(
        ":: t\n: # A\n # H\n",
        "<dl><dt>t</dt><dd><h1 id=\"A\">A</h1></dd></dl><p># H</p>",
    );
    assert_folds(
        ":: t\n: ---\n > q\n",
        "<dl><dt>t</dt><dd><hr></dd></dl><p>&gt; q</p>",
    );
}

#[test]
fn heading_folds_into_nested_item_paragraph() {
    assert_folds(
        ":: t\n: 1. para\n # H\n",
        "<dl><dt>t</dt><dd><ol><li>para\n# H</li></ol></dd></dl>",
    );
}

#[test]
fn heading_folds_into_nested_term_paragraph() {
    assert_folds(
        ":: t\n: :: t\n # H\n",
        "<dl><dt>t</dt><dd><dl><dt>t\n# H</dt></dl></dd></dl>",
    );
}

#[test]
fn thematic_break_folds_into_admonition_paragraph() {
    assert_folds(":: t\n: ::: note\n  para\n ---\n", "<dl><dt>t</dt><dd><aside class=\"admonition note\" aria-label=\"Note\"><p>para\n—</p></aside></dd></dl>");
}

#[test]
fn heading_below_description_column_inside_quote_folds() {
    assert_folds(
        "> :: t\n> : para\n>  # H\n",
        "<blockquote><dl><dt>t</dt><dd>para\n# H</dd></dl></blockquote>",
    );
}

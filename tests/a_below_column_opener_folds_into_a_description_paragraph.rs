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
        "<dl><dt>t</dt><dd>para</dd></dl><section id=\"H\"><h1>H</h1></section>",
    );
    assert_folds(
        ":: t\n:  para\n  # H\n",
        "<dl><dt>t</dt><dd>para</dd></dl><section id=\"H\"><h1>H</h1></section>",
    );
}

#[test]
fn quote_marker_below_description_column_folds() {
    assert_folds(
        ":: t\n: para\n > q\n",
        "<dl><dt>t</dt><dd>para</dd></dl><blockquote><p>q</p></blockquote>",
    );
}

#[test]
fn table_row_below_description_column_folds() {
    assert_folds(
        ":: t\n: para\n | a |\n",
        "<dl><dt>t</dt><dd>para</dd></dl><table><tbody><tr><td>a</td></tr></tbody></table>",
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
        "<dl><dt>t</dt><dd><h1 id=\"A\">A</h1></dd></dl><section id=\"H\"><h1>H</h1></section>",
    );
    assert_folds(
        ":: t\n: ---\n > q\n",
        "<dl><dt>t</dt><dd><hr></dd></dl><blockquote><p>q</p></blockquote>",
    );
}

#[test]
fn heading_folds_into_nested_item_paragraph() {
    assert_folds(
        ":: t\n: 1. para\n # H\n",
        "<dl><dt>t</dt><dd><ol><li>para</li></ol></dd></dl><section id=\"H\"><h1>H</h1></section>",
    );
}

#[test]
fn heading_folds_into_nested_term_paragraph() {
    assert_folds(
        ":: t\n: :: t\n # H\n",
        "<dl><dt>t</dt><dd><dl><dt>t</dt></dl></dd></dl><section id=\"H\"><h1>H</h1></section>",
    );
}

#[test]
fn thematic_break_folds_into_admonition_paragraph() {
    assert_folds(":: t\n: ::: note\n  para\n ---\n", "<dl><dt>t</dt><dd><aside class=\"admonition note\" aria-label=\"Note\"><p>para</p></aside></dd></dl><hr>");
}

#[test]
fn heading_below_description_column_inside_quote_folds() {
    assert_folds(
        "> :: t\n> : para\n>  # H\n",
        "<blockquote><dl><dt>t</dt><dd>para</dd></dl><h1 id=\"H\">H</h1></blockquote>",
    );
}

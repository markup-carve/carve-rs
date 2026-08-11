//! A link reference definition inside a list item is collected and invisible.
//!
//! The definition pre-pass strips blockquote markers and list MARKERS, but an
//! item's CONTINUATION line carries neither - so the item's indentation stayed
//! in front of the `[` and the line read as text. It rendered (a definition
//! renders nowhere else) and the reference it defined did not resolve.
//! carve-rs#552; carve-js, carve-php and the executable spec all collect it.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn a_definition_on_an_item_continuation_line_is_collected() {
    let out = html("- a\n\n  [r]: /u\n\nsee [text][r]\n");
    assert!(
        !out.contains("[r]: /u"),
        "definition leaked into the output:\n{out}"
    );
    assert!(out.contains(r#"<a href="/u">text</a>"#), "{out}");
}

#[test]
fn a_blank_line_before_it_changes_nothing() {
    let out = html("- a\n\n  [r]: /u\n\nsee [text][r]\n");
    assert!(!out.contains("[r]: /u"), "{out}");
    assert!(out.contains(r#"<a href="/u">text</a>"#), "{out}");
}

#[test]
fn past_the_content_column_it_is_item_text() {
    // Indented PAST the column, the line is the item's paragraph text: it
    // renders, and it defines nothing. All four implementations agree here,
    // which is why the fix strips exactly the content column and never more.
    let out = html("- a\n      [r]: /u\n\nsee [t][r]\n");
    assert!(out.contains("[r]: /u"), "{out}");
    assert!(out.contains("see [t][r]"), "{out}");
}

#[test]
fn at_the_top_level_an_indented_definition_is_paragraph_text() {
    // `text` / `  [r]: /u` folds as lazy continuation everywhere, so the
    // content-column strip must not fire outside a list item.
    let out = html("text\n  [r]: /u\n\nsee [t][r]\n");
    assert!(out.contains("[r]: /u"), "{out}");
    assert!(out.contains("see [t][r]"), "{out}");
}

#[test]
fn a_blockquote_definition_still_works() {
    // The path that was already correct, kept honest: the fix touches the
    // no-marker continuation case only.
    let out = html("> a\n>\n> [r]: /u\n\nsee [text][r]\n");
    assert!(!out.contains("[r]: /u"), "{out}");
    assert!(out.contains(r#"<a href="/u">text</a>"#), "{out}");
}

#[test]
fn a_footnote_definition_at_the_content_column_is_collected() {
    // The same gap as the link-reference case above, in the OTHER prepass: an
    // item's continuation line carries no marker, so the indentation stayed in
    // front of the `[` and the definition was neither collected nor rendered -
    // the author's line disappeared and a reference to it stayed literal
    // (carve-rs#568). carve-js and the executable spec both collect it.
    let out = carve::to_html("- a\n\n  [^f]: x\n\nsee[^f]\n");
    assert!(!out.contains("[^f]: x"), "definition leaked:\n{out}");
    assert!(
        out.contains("doc-noteref"),
        "reference did not resolve:\n{out}"
    );
    assert!(out.contains("doc-endnotes"), "no endnotes section:\n{out}");
}

#[test]
fn a_footnote_definition_below_the_content_column_is_text() {
    // One column left it is outside the item body: it folds as paragraph text
    // and registers nothing, which is what carve#624 settled for every
    // definition kind. The strip must be exactly the content column so these
    // two cases stay apart.
    let out = carve::to_html("- a\n [^f]: x\n\nsee[^f]\n");
    assert!(out.contains("[^f]: x"), "{out}");
    assert!(
        out.contains("see[^f]"),
        "reference should stay literal:\n{out}"
    );
}

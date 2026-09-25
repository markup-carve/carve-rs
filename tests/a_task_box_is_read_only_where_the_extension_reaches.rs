//! A box is read only where cmark-gfm's tasklist extension reaches (carve-rs#1899).
//!
//! pulldown reads a box off any bullet item opening with a bracket pair. The
//! extension is narrower, and cmark-gfm 0.29.0.gfm.13 is the reader the importers
//! answer to (markup-carve/carve#2187), so a quoted or nested list used to grow a
//! box the reader has none of.
//!
//! Every case asserts the RENDERED HTML as well as the Carve, because a pair kept
//! as text and a box look alike in the Carve and are unmistakable in the HTML.
//! Each expected reading was measured against cmark-gfm through the spec repo's
//! `markdown-oracle.mjs`, not recalled.

use carve::{markdown_to_carve, to_html};

fn imported(markdown: &str) -> (String, String) {
    let carve = markdown_to_carve(markdown);
    let html = to_html(&carve);
    (carve, html)
}

fn assert_text(markdown: &str, carve_source: &str, item: &str) {
    let (carve, html) = imported(markdown);
    assert_eq!(carve, carve_source);
    assert!(!html.contains("type=\"checkbox\""), "{html}");
    assert!(html.contains(item), "{html}");
}

fn assert_box(markdown: &str, label: &str) {
    let (carve, html) = imported(markdown);
    assert!(
        html.contains(&format!(
            r#"<input type="checkbox" disabled aria-label="{label}">"#
        )),
        "{carve}{html}"
    );
}

#[test]
fn a_quoted_list_keeps_its_bracket_pair_as_text() {
    assert_text("> - [ ] foo\n", "> - \\[ ] foo\n", "<li>[ ] foo</li>");
    assert_text("> - [x] foo\n", "> - \\[x] foo\n", "<li>[x] foo</li>");
}

#[test]
fn a_list_nested_on_the_same_line_keeps_its_bracket_pair_as_text() {
    assert_text("- - [ ] foo\n", "- - \\[ ] foo\n", "<li>[ ] foo</li>");
    assert_text("- - [x] foo\n", "- - \\[x] foo\n", "<li>[x] foo</li>");
}

#[test]
fn a_top_level_bullet_still_imports_a_box() {
    // THE CONTROL. Without it a change that simply stopped reading boxes passes
    // every case above.
    let (carve, html) = imported("- [ ] foo\n");
    assert_eq!(carve, "- [ ] foo\n");
    assert!(
        html.contains(r#"<input type="checkbox" disabled aria-label="foo">"#),
        "{html}"
    );
}

#[test]
fn indentation_alone_never_takes_the_box_away() {
    // The shapes a nesting-depth rule would get wrong: the extension counts the
    // markers on the LINE, so a sublist that opens on a line of its own keeps
    // its box however deep it sits.
    assert_box("   - [ ] foo\n", "foo");
    assert_box("- a\n  - [ ] foo\n", "foo");
    assert_box("- a\n  - b\n    - [ ] foo\n", "foo");
    assert_box("- - a\n    - [ ] foo\n", "foo");
    assert_box("1. a\n   - [ ] foo\n", "foo");
}

#[test]
fn a_quote_marker_on_the_line_takes_the_box_away_even_from_a_sublist() {
    // The mirror of the case above, and the pair that shows the rule is about
    // the line rather than the nesting: both sublists open on their own line,
    // and only the quoted one loses its box.
    assert_text(
        "> - a\n>   - [ ] foo\n",
        "> - a\n>   - \\[ ] foo\n",
        "<li>[ ] foo</li>",
    );
    assert_text(
        "- a\n  > - [ ] foo\n",
        "- a\n  > - \\[ ] foo\n",
        "<li>[ ] foo</li>",
    );
}

#[test]
fn a_bullet_nested_in_an_ordered_item_on_the_same_line_keeps_its_text() {
    assert_text("1. - [ ] foo\n", "1. - \\[ ] foo\n", "<li>[ ] foo</li>");
}

#[test]
fn a_pair_that_ends_its_line_is_text() {
    // The extension wants whitespace after the pair on the SAME line. `- [ ]`
    // alone needs no escape, since Carve reads no task item without content.
    assert_text("- [ ]\n", "- [ ]\n", "<li>[ ]</li>");
    assert_text("- [x]\n", "- [x]\n", "<li>[x]</li>");
    // The label on the next line stays a soft break away from the brackets
    // rather than joining them.
    assert_text("- [ ]\n  foo\n", "- [ ]\n  foo\n", "<li>[ ]\nfoo</li>");
}

#[test]
fn a_pair_holding_a_tab_is_text() {
    // pulldown takes any horizontal whitespace as the unchecked state; the
    // extension takes a literal space.
    assert_text("- [\t] foo\n", "- [\t] foo\n", "<li>[\t] foo</li>");
}

#[test]
fn a_quoted_list_with_no_bracket_pair_is_unchanged() {
    // THE CONTROL ON THE OTHER SIDE, twice: the change must reach a pair that
    // opens an item and nothing else in a quoted list.
    let (carve, html) = imported("> - foo\n");
    assert_eq!(carve, "> - foo\n");
    assert!(html.contains("<li>foo</li>"), "{html}");
    let (carve, html) = imported("> - a [ ] b\n");
    assert_eq!(carve, "> - a [ ] b\n");
    assert!(html.contains("<li>a [ ] b</li>"), "{html}");
}

#[test]
fn an_ordered_task_item_still_keeps_its_bracket_text() {
    // carve-rs#1891's reading, unchanged: the extension DOES reach here, and
    // Carve has no ordered task item to spell it as.
    let (carve, html) = imported("1. [x] done\n");
    assert_eq!(carve, "1. [x] done\n");
    assert!(html.contains("<li>[x] done</li>"), "{html}");
}

#[test]
fn the_four_carve_only_states_are_unchanged() {
    // They were already text, which is what cmark-gfm reads - the box carve-js
    // and carve-php grow there (markup-carve/carve-js#2048) was never grown
    // here, and this pins that the change did not start.
    for (markdown, item) in [
        ("- [-] foo\n", "<li>[-] foo</li>"),
        ("- [?] foo\n", "<li>[?] foo</li>"),
        ("- [/] foo\n", "<li>[/] foo</li>"),
        ("- [*] foo\n", "<li>[*] foo</li>"),
    ] {
        let (carve, html) = imported(markdown);
        assert!(!html.contains("type=\"checkbox\""), "{carve}{html}");
        assert!(html.contains(item), "{html}");
    }
}

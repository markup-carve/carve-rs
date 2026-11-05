//! carve-rs#3883: `<hr data-char="_">` imports as `___` (PART 10, "A RESERVED
//! NAME NEVER LEAVES AS AN AUTHORED ATTRIBUTE"). `carve fmt` still writes
//! every break `***` (PART 11 §6).

use carve::{html_to_carve, parse, render_carve, HtmlImportOptions};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

#[test]
fn the_recorded_break_character_is_written() {
    assert_eq!(imported("<hr data-char=\"_\">"), "___\n");
    assert_eq!(imported("<hr data-char=\"*\">"), "***\n");
    assert_eq!(
        imported("<p>a</p><hr data-char=\"_\"><p>b</p>"),
        "a\n\n___\n\nb\n"
    );
    assert_eq!(
        imported("<ul><li><p>a</p><hr data-char=\"_\"></li></ul>"),
        imported("<ul><li><p>a</p><hr></li></ul>").replace("***", "___"),
    );
}

#[test]
fn a_break_without_a_usable_character_stays_canonical() {
    assert_eq!(imported("<hr>"), "***\n");
    assert_eq!(imported("<hr data-char=\"x\">"), "***\n");
}

#[test]
fn a_dash_is_kept_unless_it_would_open_frontmatter() {
    assert_eq!(imported("<p>a</p><hr data-char=\"-\">"), "a\n\n---\n");
    assert_eq!(
        imported("<hr data-char=\"-\"><p>a</p><hr data-char=\"_\">"),
        "***\n\na\n\n***\n",
    );
}

#[test]
fn fmt_still_writes_every_break_canonically() {
    assert_eq!(render_carve(&parse("___\n")).unwrap(), "***\n");
}

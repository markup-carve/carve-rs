//! A fence title needs a language in front of it.
//!
//! In Markdown the info string's FIRST TOKEN is the language, so a title emitted
//! without one is read as the language: commonmark.js turns
//! ``` ``` "notes.txt" ``` into `class="language-&quot;notes.txt&quot;"`. Markdown
//! cannot express a fence title on its own, so dropping it beats emitting a bogus
//! language class. carve-php had this guard and was right about it (carve#352,
//! corpus 11-fenced-code-8).

fn info_string(src: &str) -> String {
    carve::to_markdown(src)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_title_without_a_language_is_dropped() {
    assert_eq!(
        info_string("``` \"notes.txt\"\nremember the milk\n```\n"),
        "```"
    );
}

#[test]
fn a_title_with_a_language_is_kept() {
    assert_eq!(
        info_string("```php \"notes.php\"\ncode\n```\n"),
        "```php \"notes.php\""
    );
}

#[test]
fn a_grouping_label_survives_without_a_language() {
    // A label is bracketed, so it cannot be mistaken for a language token.
    assert_eq!(info_string("``` [Build]\ncode\n```\n"), "``` [Build]");
}

#[test]
fn language_title_and_label_all_ride_together() {
    assert_eq!(
        info_string("```php \"f.php\" [Build]\ncode\n```\n"),
        "```php \"f.php\" [Build]"
    );
}

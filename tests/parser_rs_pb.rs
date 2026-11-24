//! The fence-title-syntax message for a code fence (carve-rs#4725) and the blank
//! lines an opaque body held above a comment line that ends it (carve-rs#4726).

use carve::lint::{LintOptions, lint};
use carve::{Options, render};

fn title_message(opener: &str) -> String {
    let source = format!("{opener}\ncode\n```\n");
    let found: Vec<String> = lint(&source, &LintOptions::default())
        .into_iter()
        .filter(|d| d.code == "fence-title-syntax")
        .map(|d| d.message)
        .collect();
    assert_eq!(found.len(), 1, "{opener:?}: {found:?}");
    found.into_iter().next().unwrap_or_default()
}

fn generic(corrected: &str) -> String {
    format!(
        "These further words are not read as a title, label or attributes. The block opens without them. Use \"{corrected}\" for the same block with only valid opener parts; put attributes on their own line above it."
    )
}

#[test]
fn c2_b_122_unquoted_title_after_a_code_fence_language() {
    for (opener, write) in [
        ("``` js x y", "``` js \"x y\""),
        ("```js b", "``` js \"b\""),
        ("``` js  x", "``` js \"x\""),
        ("``` js x [l]", "``` js \"x\" [l]"),
        ("``` js \"t", "``` js \"t\""),
        ("``` js ‘t’", "``` js \"‘t’\""),
        ("- ``` c++ x", "``` c++ \"x\""),
    ] {
        let lang = if opener.contains("c++") { "c++" } else { "js" };
        assert_eq!(
            title_message(opener),
            format!(
                "Text after \"{lang}\" must be a quoted title or a [label]; it is not read as a title. Write {write}."
            ),
            "{opener:?}"
        );
    }
}

#[test]
fn c2_b_122_typographic_quotes_after_a_code_fence_language() {
    for (opener, write) in [
        ("``` js “t”", "``` js \"t\""),
        ("``` js “t” [l]", "``` js \"t\" [l]"),
        ("``` js “t", "``` js \"t\""),
        ("> ```` js „t“", "```` js \"t\""),
    ] {
        assert_eq!(
            title_message(opener),
            format!(
                "The title after \"js\" uses typographic quotes. Check whether a smart quote filter changed the source; a fence title needs straight double quotes: {write}."
            ),
            "{opener:?}"
        );
    }
}

#[test]
fn c2_b_122_other_further_words_keep_their_messages() {
    let after_title = "These words follow a valid title and are not part of the code fence opener. Remove them or put them in the block body.";
    assert_eq!(title_message("``` js \"t\" x"), after_title);
    assert_eq!(title_message("``` js \"t\"\tx"), after_title);
    // A title after a label, a raw format, a spaced brace block, a glued character.
    assert_eq!(title_message("``` js [l] \"t\""), generic("``` js [l]"));
    assert_eq!(title_message("``` =html x"), generic("``` =html"));
    assert_eq!(title_message("```=html x"), generic("``` =html"));
    assert_eq!(title_message("```js {.a}"), generic("``` js"));
    assert_eq!(title_message("``` js“t”"), generic("``` js"));
    assert_eq!(
        title_message("``` \tx"),
        "The whitespace in this fence opener is invalid: at most one space follows the fence, and info-string parts use spaces, never tabs. The block opens with no language; write \"``` x\"."
    );
    assert_eq!(
        title_message("```js\tx"),
        "The tab does not separate valid info-string parts; the block keeps its language, but does not keep the later title or label. Use \"``` js x\" with spaces between parts."
    );
    assert_eq!(
        title_message("``` js x\ty"),
        "Text after \"js\" must be a quoted title or a [label]; it is not read as a title. Write ``` js \"x\ty\"."
    );
}

const LOOSE_AFTER_QUOTE: &str = "<ul>\n  <li>\n    <blockquote>\n      <div class=\"line-block\">\n        <p>a</p>\n      </div>\n    </blockquote>\n  </li>\n  <li><p>y</p></li>\n</ul>";

#[test]
fn c2_b_137_a_comment_line_ending_a_body_keeps_the_blank_lines_it_held() {
    let opts = Options::default();
    assert_eq!(
        render("- > ::: |\n  > a\n  >\n%% c\n- y\n", &opts),
        LOOSE_AFTER_QUOTE
    );
    assert_eq!(
        render("- > ::: |\n  > a\n  >\n  %% c\n- y\n", &opts),
        LOOSE_AFTER_QUOTE
    );
    assert_eq!(
        render("- > ```\n  > a\n  >\n%% c\n- y\n", &opts),
        "<ul>\n  <li>\n    <blockquote>\n      <pre><code>a\n</code></pre>\n    </blockquote>\n  </li>\n  <li><p>y</p></li>\n</ul>"
    );
    // No blank line held: the list stays tight.
    assert_eq!(
        render("- > ::: |\n  > a\n%% c\n- y\n", &opts),
        "<ul>\n  <li>\n    <blockquote>\n      <div class=\"line-block\">\n        <p>a</p>\n      </div>\n    </blockquote>\n  </li>\n  <li>y</li>\n</ul>"
    );
}

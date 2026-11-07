//! AN ATTACHED LINE BLOCK OR QUOTE BLOCK ENDS AT ITS FIRST BLANK
//! (markup-carve/carve#5414).
//!
//! In a footnote or description host, an unclosed colon body a marker-line `+`
//! takes ends at its first blank (PART 0 S1, §17 L3). That held for a div but
//! not for a line block (`::: |`) or a quote block (`::: >`), which ran on past
//! the blank. A wider run (`::::`) does not close a `:::` line block.
//!
//! Expectations are the executable spec's output at markup-carve/carve 4293e8a5.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

fn note_item(html: &str) -> &str {
    let start = html.find("<li id=\"fn1\">").expect("footnote item");
    let end = html[start..].find("</li>").expect("item end") + start;
    &html[start..end]
}

#[test]
fn a_footnote_line_block_ends_at_its_first_blank() {
    let html = both_paths("[^1]: +\n::: |\n  a\n\n  b\n\n[^1]\n");
    assert_eq!(
        note_item(&html),
        concat!(
            "<li id=\"fn1\">\n",
            "      <div class=\"line-block\">\n",
            "        <p>&nbsp;&nbsp;a</p>\n",
            "      </div>\n",
            "      <p>b<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n",
            "    ",
        ),
    );
}

#[test]
fn a_footnote_quote_block_ends_at_its_first_blank() {
    let html = both_paths("[^1]: +\n::: >\n  a\n\n  b\n\n[^1]\n");
    assert_eq!(
        note_item(&html),
        concat!(
            "<li id=\"fn1\">\n",
            "      <blockquote><p>a</p></blockquote>\n",
            "      <p>b<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n",
            "    ",
        ),
    );
}

#[test]
fn a_wider_run_after_the_blank_opens_a_div() {
    let html = both_paths("[^1]: +\n::: |\n\n  ::::\n\n[^1]\n");
    assert_eq!(
        note_item(&html),
        concat!(
            "<li id=\"fn1\">\n",
            "      <div class=\"line-block\">\n",
            "\n",
            "      </div>\n",
            "      <div>\n",
            "\n",
            "      </div>\n",
            "      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n",
            "    ",
        ),
    );
}

#[test]
fn a_closer_in_the_host_still_closes_the_line_block() {
    let html = both_paths("[^1]: +\n::: |\n\n  ::: x\n  :::\n\n[^1]\n");
    assert!(
        note_item(&html).contains("<div class=\"line-block\">\n        <p>::: x</p>\n      </div>"),
        "{html}"
    );
}

#[test]
fn a_description_quote_block_ends_at_its_first_blank() {
    assert!(
        both_paths(":: t\n: +\n::: >\n  a\n\n  b\n").contains(concat!(
            "  <dd>\n",
            "    <blockquote><p>a</p></blockquote>\n",
            "    <p>b</p>\n",
            "  </dd>",
        )),
    );
}

#[test]
fn a_run_after_the_footnote_ends_closes_nothing_in_it() {
    // The host ends at the column-0 reference, so the later `:::` is not its closer.
    let html = both_paths("[^1]: +\n::: >\n  a\n\n  b\n\n[^1]\n\n:::\n");
    assert!(
        note_item(&html).contains("<blockquote><p>a</p></blockquote>\n      <p>b<a"),
        "{html}"
    );
}

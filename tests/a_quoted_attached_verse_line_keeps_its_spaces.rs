//! A QUOTED ATTACHED VERSE LINE KEEPS ITS SPACES (markup-carve/carve#5429).
//!
//! A block a marker-line `+` takes in a quoted item is pulled at the content
//! column, but never past its own first line, so a line block's verse keeps the
//! spaces it has beyond that line, as it does outside the quote. And a closer
//! the host holds past the first blank closes the body at whatever column it
//! stands (carve#5405).
//!
//! Expectations are the executable spec's output at markup-carve/carve#5412.

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

#[test]
fn a_quoted_verse_line_keeps_its_leading_spaces() {
    assert_eq!(
        both_paths("> - +\n> ::: |\n>   a\n").trim(),
        concat!(
            "<blockquote>\n",
            "  <ul>\n",
            "    <li>\n",
            "      <div class=\"line-block\">\n",
            "        <p>&nbsp;&nbsp;a</p>\n",
            "      </div>\n",
            "    </li>\n",
            "  </ul>\n",
            "</blockquote>",
        ),
    );
}

#[test]
fn a_deeper_host_closer_closes_the_line_block() {
    assert_eq!(
        both_paths("- +\n::: |\n    a\n\n    b\n    :::\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <div class=\"line-block\">\n",
            "      <p>&nbsp;&nbsp;&nbsp;&nbsp;a</p>\n",
            "      <p>&nbsp;&nbsp;b</p>\n",
            "    </div>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

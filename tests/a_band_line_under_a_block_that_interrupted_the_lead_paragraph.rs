//! A BAND LINE UNDER A BLOCK THAT INTERRUPTED THE ITEM'S LEAD PARAGRAPH
//! (markup-carve/carve-rs#2128).
//!
//! The third branch of the family markup-carve/carve-rs#2120 settled. There the
//! block is the item's marker-line content; here it interrupted the lead
//! paragraph and is collected as a chunk, with the list's base as the collection
//! floor - so the band line was taken before anything asked whether the item
//! could hold it.
//!
//! The chunk's collection floor is what asks. Raised, the band line is left for
//! the fold below it, which hands it back when the chunk holds an open paragraph;
//! left behind, the end-of-list check under the fold reads it. Asking the fold
//! rather than the chunk's first line is what settles a fence or a container
//! CLOSED inside the chunk, whose opener is that first line.
//!
//! A comment is column-exempt and keeps the item either way (PART 9 S24 C3).
//!
//! Expectations are the executable spec's output, run per document against
//! `scripts/spec/layout.mjs` plus `scripts/spec/html.mjs` at markup-carve/carve
//! 66d4ed19.

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
fn the_reported_reduction_leaves_the_band_line_at_document_level() {
    // The heading interrupted the item's LEAD paragraph, so it is the chunk's first
    // line rather than the marker line's content. It leaves nothing open, so the
    // band line has nothing here to continue.
    assert_eq!(
        both_paths(". t\n  # g\n z\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>t\n",
            "    <h1 id=\"g\">g</h1>\n",
            "  </li>\n",
            "</ol>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_thematic_break_answers_the_same_way() {
    assert_eq!(
        both_paths("- t\n  ---\n z\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>t\n",
            "    <hr>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_table_answers_the_same_way() {
    assert_eq!(
        both_paths("- t\n  | a |\n  | --- |\n z\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>t\n",
            "    <table>\n",
            "      <thead>\n",
            "        <tr><th scope=\"col\">a</th></tr>\n",
            "      </thead>\n",
            "    </table>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_code_fence_closed_inside_the_chunk_leaves_nothing_open() {
    // The fence's OPENER is the chunk's first line, and a predicate asked of that
    // line alone reads an open fence. The fold asks the whole chunk instead, which
    // is what sees the closer.
    assert_eq!(
        both_paths("- t\n  ```c\n  x\n  ```\n z\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>t\n",
            "    <pre><code class=\"language-c\">x\n",
            "</code></pre>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_container_closed_inside_the_chunk_leaves_nothing_open() {
    assert_eq!(
        both_paths("- t\n  ::: d\n  b\n  :::\n z\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>t\n",
            "    <div class=\"d\">\n",
            "      <p>b</p>\n",
            "    </div>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_wide_marker_widens_the_band_it_applies_to() {
    // The band is the list's base to the item's content column, so a `10. ` item has
    // three columns of it and a line in any of them answers the same.
    assert_eq!(
        both_paths("10. t\n    # g\n  z\n").trim(),
        concat!(
            "<ol start=\"10\">\n",
            "  <li>t\n",
            "    <h1 id=\"g\">g</h1>\n",
            "  </li>\n",
            "</ol>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_paragraph_at_the_column_still_takes_the_band_line() {
    // THE CONTROL the raised floor could break. The fold below the collection is what
    // hands the line back, and it is the reason the floor may be raised blind.
    assert_eq!(
        both_paths("- t\n  p\n z\n").trim(),
        concat!("<ul>\n", "  <li>t\n", "p\n", "z</li>\n", "</ul>",),
    );
}

#[test]
fn an_open_fence_still_owns_the_band_line_as_payload() {
    assert_eq!(
        both_paths("- t\n  ```c\n z\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>t\n",
            "<code>c\n",
            "z</code></li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn an_open_container_still_ends_the_item() {
    assert_eq!(
        both_paths("- t\n  ::: d\n z\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>t\n",
            "    <div class=\"d\">\n",
            "\n",
            "    </div>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_line_comment_is_column_exempt_and_keeps_the_item() {
    // PART 9 S24 C3. A comment renders nothing at any column, so it closed nothing and
    // the lead paragraph is still there for the band line to continue.
    assert_eq!(
        both_paths("- t\n  %% c\n z\n").trim(),
        concat!("<ul>\n", "  <li>t\n", "    z\n", "  </li>\n", "</ul>",),
    );
}

#[test]
fn a_comment_fence_is_column_exempt_too() {
    assert_eq!(
        both_paths("- t\n  %%%\n  h\n  %%%\n z\n").trim(),
        concat!("<ul>\n", "  <li>t\n", "    z\n", "  </li>\n", "</ul>",),
    );
}

#[test]
fn column_zero_ends_the_list_through_the_ordinary_dedent() {
    // THE EDGES, neither of which the band covers.
    assert_eq!(
        both_paths("- t\n  # g\nz\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>t\n",
            "    <h1 id=\"g\">g</h1>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>z</p>",
        ),
    );
}

#[test]
fn a_line_at_the_content_column_reaches_the_item() {
    assert_eq!(
        both_paths("- t\n  # g\n  z\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>t\n",
            "    <h1 id=\"g\">g</h1>\n",
            "    z\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

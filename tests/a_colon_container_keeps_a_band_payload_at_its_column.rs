//! A COLON CONTAINER BASED PAST ITS HOST KEEPS A BAND PAYLOAD
//! (markup-carve/carve-rs#2095).
//!
//! CARVE-P0-004: a `:::` container opened past its host's content column takes
//! its authored column as the local base, and only that base or the host's own
//! column closes it. A run strictly between the two reaches neither, so it is
//! payload. markup-carve/carve-rs#2104 settled the closer, and left the case
//! where the PAYLOAD sits in the band too: `rebase_overindented_blocks` runs
//! twice over one item body - `item_body` then `parse_item_chunk` - and the
//! first run correctly leaves the band lines at their authored column, so the
//! second read the in-band `:::` as an authored-base opener, dedented it onto
//! the container's column, and the container took it as its closer.
//!
//! Expectations are the executable spec's output, run per document against
//! `scripts/spec/layout.mjs` plus `scripts/spec/html.mjs` at markup-carve/carve
//! 774eb404, and cross-read against carve-js 6c4b882b1 and carve-php 4efa406bc,
//! which agree on every document here.

use carve::{to_html, to_html_with_options, Options};

/// The position-tracking path is a second parse of the same source, and a parse
/// that answers differently once positions are on is a defect on its own
/// (the standing guard from markup-carve/carve-rs#1511).
fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

const IN_BAND: &str = concat!(
    "<ul>\n",
    "  <li>a\n",
    "    <div>\n",
    "      <p>p\n",
    ":::</p>\n",
    "      <p>tail</p>\n",
    "    </div>\n",
    "  </li>\n",
    "</ul>",
);

#[test]
fn the_reported_shapes_keep_the_band_run_as_payload() {
    // The closer sits at column 4 in the first and column 2 in the second -
    // both inside the band between the item's content column (3) and the
    // container's authored base (5).
    for src in [
        "- a\n\n    :::\n   p\n   :::\n  \n  tail\n",
        "- a\n\n    :::\n   p\n :::\n  \n  tail\n",
    ] {
        assert_eq!(both_paths(src).trim(), IN_BAND, "{src:?}");
    }
}

#[test]
fn an_ordered_host_and_an_admonition_kind_read_the_same_way() {
    assert_eq!(
        both_paths("1. a\n\n     :::\n    p\n    :::\n   \n   tail\n").trim(),
        concat!(
            "<ol>\n",
            "  <li>a\n",
            "    <div>\n",
            "      <p>p\n",
            ":::</p>\n",
            "      <p>tail</p>\n",
            "    </div>\n",
            "  </li>\n",
            "</ol>",
        ),
    );
    assert_eq!(
        both_paths("- a\n\n    ::: note\n   p\n   :::\n  \n  tail\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>a\n",
            "    <aside class=\"admonition note\" aria-label=\"Note\">\n",
            "      <p>p\n",
            ":::</p>\n",
            "      <p>tail</p>\n",
            "    </aside>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_payload_at_the_base_was_already_right() {
    // markup-carve/carve-rs#2104's half: with the payload at the base and the
    // closer in the band, the run is still payload. Kept here because the two
    // halves share the extent walk this change touches.
    assert_eq!(
        both_paths("- a\n\n    :::\n    p\n   :::\n  \n  tail\n").trim(),
        IN_BAND,
    );
}

#[test]
fn a_closer_at_the_base_still_closes() {
    // THE CONTROL. Payload in the band, closer AT the authored base: the base
    // closes, so the div holds `p` alone and `tail` is the item's own paragraph.
    // A fix that kept every band-adjacent colon run as payload would take this.
    assert_eq!(
        both_paths("- a\n\n    :::\n   p\n    :::\n  \n  tail\n").trim(),
        concat!(
            "<ul>\n",
            "  <li><p>a</p>\n",
            "    <div>\n",
            "      <p>p</p>\n",
            "    </div>\n",
            "    <p>tail</p>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_container_at_the_host_column_closes_at_that_column() {
    // THE OTHER CONTROL: with no band at all - base == host column - the closer
    // reaches the container and nothing about this changes.
    assert_eq!(
        both_paths("- a\n\n  :::\n  p\n  :::\n  \n  tail\n").trim(),
        concat!(
            "<ul>\n",
            "  <li><p>a</p>\n",
            "    <div>\n",
            "      <p>p</p>\n",
            "    </div>\n",
            "    <p>tail</p>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_nested_width_inside_the_extent_still_pairs() {
    // The extent walk tracks colon WIDTHS as a stack, so an inner `::::` at the
    // base pairs with its own closer and the band payload between them belongs
    // to the inner container.
    assert_eq!(
        both_paths("- a\n\n    :::\n    ::::\n   p\n    ::::\n    :::\n  \n  tail\n").trim(),
        concat!(
            "<ul>\n",
            "  <li><p>a</p>\n",
            "    <div>\n",
            "      <div>\n",
            "        <p>p</p>\n",
            "      </div>\n",
            "    </div>\n",
            "    <p>tail</p>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_description_host_and_the_code_fence_spelling_are_unmoved() {
    // A description body reaches the same reading through its own collector, and
    // the code fence is the arm the colon arm mirrors - both were already right,
    // and this pass rewrites the extent both of them walk.
    assert_eq!(
        both_paths(":: term\n:  desc\n\n    :::\n   p\n   :::\n   \n   tail\n").trim(),
        concat!(
            "<dl>\n",
            "  <dt>term</dt>\n",
            "  <dd>\n",
            "    <p>desc</p>\n",
            "    <div>\n",
            "      <p>p</p>\n",
            "    </div>\n",
            "    <p>tail</p>\n",
            "  </dd>\n",
            "</dl>",
        ),
    );
    assert_eq!(
        both_paths("- a\n\n    ```\n   p\n   ```\n  \n  tail\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>a\n",
            "    <pre><code> p\n",
            " ```\n",
            "\n",
            "tail\n",
            "</code></pre>\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

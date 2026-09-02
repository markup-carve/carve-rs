//! A DEGRADED COMMENT FENCE AT A DESCENDANT'S CONTENT COLUMN ENDS THAT
//! DESCENDANT'S ITEM AND EVERY ITEM ABOVE IT (markup-carve/carve-rs#1518).
//!
//! PART 9 §28 degrades a comment fence with no closer to a line comment, which
//! leaves no paragraph open - so a line below the container's content column
//! continues nothing and the container ends there (carve-rs#1512). A marker
//! ladder (`- - - x`) re-parses a DEDENTED copy of the same chunk at every
//! level, each frame collecting against its OWN content column, so a fence at
//! the innermost column was at `strip_cols` only in the innermost frame. The
//! frames above it kept collecting, and the line under the fence landed one
//! item too deep at depth three and two too deep at depth four.
//!
//! TWO BANDS, TWO QUESTIONS. A fence at the frame's own column ends the item
//! for a line below THAT column - local arithmetic, unchanged. A fence at a
//! DESCENDANT's column closed the descendant's paragraph, not this frame's, so
//! this frame ends only for a line that reached NOTHING in the document. The
//! dedent destroys the column a frame would need to tell those apart, which is
//! what `LineCursor::reached` now carries down.
//!
//! ORACLE: the executable spec (`tests/spec/scripts/spec/layout.mjs` +
//! `html.mjs`) at carve `95fc3a04`, which is BOTH the pinned submodule and spec
//! main - `markup-carve/carve#1902`'s oracle fix for the comment column
//! exemption is an ancestor of it, so no pin/main split applies. Every
//! expectation below is that oracle's own output.

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

/// A depth-three ladder holding `body` in the OUTERMOST item.
fn outermost(body: &str) -> String {
    format!(
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>\n",
            "        <ul>\n",
            "          <li>x</li>\n",
            "        </ul>\n",
            "      </li>\n",
            "    </ul>\n",
            "    {}\n",
            "  </li>\n",
            "</ul>",
        ),
        body
    )
}

#[test]
fn the_reported_document_puts_the_line_in_the_outermost_item() {
    assert_eq!(both_paths("- - - x\n      %%% x\n # h\n"), outermost("# h"));
}

#[test]
fn every_fence_width_answers_the_same() {
    for width in ["%%%", "%%%%", "%%%%%"] {
        let src = format!("- - - x\n      {width} x\n # h\n");
        assert_eq!(both_paths(&src), outermost("# h"), "{src:?}");
    }
}

#[test]
fn the_kind_of_the_line_does_not_matter() {
    // It reached nothing, so it is text of the outermost item whatever it looks
    // like. A thematic break rendered an `<hr>` and a quote a `<blockquote>`,
    // each two items too deep.
    for (line, folded) in [
        ("---", "—"),
        ("> q", "&gt; q"),
        ("| a |", "| a |"),
        ("::: note", "::: note"),
        ("{.k}", "{.k}"),
    ] {
        let src = format!("- - - x\n      %%% x\n {line}\n");
        assert_eq!(both_paths(&src), outermost(folded), "{src:?}");
    }
}

#[test]
fn depth_four_answers_the_same() {
    // The answer is the OUTERMOST item at every depth, not "one item up".
    assert_eq!(
        both_paths("- - - - x\n        %%% x\n # h\n"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>\n",
            "        <ul>\n",
            "          <li>\n",
            "            <ul>\n",
            "              <li>x</li>\n",
            "            </ul>\n",
            "          </li>\n",
            "        </ul>\n",
            "      </li>\n",
            "    </ul>\n",
            "    # h\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_fence_at_an_intermediate_ladder_column_ends_the_same_items() {
    // EVERY DESCENDANT COLUMN COUNTS, not only the innermost. A depth-four
    // ladder opens 2, 4, 6 and 8; a fence at 6 is the depth-three item's
    // column, and keeping only the innermost one in
    // `marker_ladder_descendant_columns` leaves this line in the depth-three
    // item.
    assert_eq!(
        both_paths("- - - - x\n      %%% x\n # h\n"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>\n",
            "        <ul>\n",
            "          <li>\n",
            "            <ul>\n",
            "              <li>x</li>\n",
            "            </ul>\n",
            "          </li>\n",
            "        </ul>\n",
            "      </li>\n",
            "    </ul>\n",
            "    # h\n",
            "  </li>\n",
            "</ul>",
        ),
    );
}

#[test]
fn a_quote_host_ends_at_its_own_outermost_item() {
    assert_eq!(
        both_paths("> - - - x\n>       %%% x\n>  # h\n"),
        concat!(
            "<blockquote>\n",
            "  <ul>\n",
            "    <li>\n",
            "      <ul>\n",
            "        <li>\n",
            "          <ul>\n",
            "            <li>x</li>\n",
            "          </ul>\n",
            "        </li>\n",
            "      </ul>\n",
            "      # h\n",
            "    </li>\n",
            "  </ul>\n",
            "</blockquote>",
        ),
    );
}

#[test]
fn a_line_that_reached_an_item_stays_in_it() {
    // THE CONTROL THE CARRIED `reached` FLAG EXISTS FOR. Column 3 reaches the
    // outermost item's column 2 and column 5 reaches the depth-two item's
    // column 4; after the collector's dedent both arrive in the nested frame at
    // the SAME residual column as the column-1 line above, and only the carried
    // flag separates them. Ask `indent < strip_cols` in the descendant band
    // instead and all three of these move an item outwards.
    assert_eq!(
        both_paths("- - - x\n      %%% x\n   # h\n"),
        outermost("<h1 id=\"h\">h</h1>")
    );
    let depth_two_b = concat!(
        "<ul>\n",
        "  <li>\n",
        "    <ul>\n",
        "      <li>\n",
        "        <ul>\n",
        "          <li>x</li>\n",
        "        </ul>\n",
        "        b\n",
        "      </li>\n",
        "    </ul>\n",
        "  </li>\n",
        "</ul>",
    );
    assert_eq!(both_paths("- - - x\n      %%% x\n   b\n"), depth_two_b);
    assert_eq!(both_paths("- - - x\n      %%% x\n     b\n"), depth_two_b);
}

#[test]
fn a_fence_at_the_frames_own_column_still_uses_the_local_column() {
    // THE CONTROL FOR THE OTHER BAND (carve-rs#1512). At depth two the fence
    // sits at the innermost frame's own `strip_cols`, and a column-3 line below
    // that column ends the item there - the carried flag must NOT be consulted
    // here, or this line stays in the item it left.
    let outer = concat!(
        "<ul>\n",
        "  <li>\n",
        "    <ul>\n",
        "      <li>x</li>\n",
        "    </ul>\n",
        "    {}\n",
        "  </li>\n",
        "</ul>",
    );
    assert_eq!(
        both_paths("- - x\n    %%% x\n   b\n"),
        outer.replace("{}", "b")
    );
    assert_eq!(
        both_paths("- - x\n    %%% x\n # h\n"),
        outer.replace("{}", "# h")
    );
}

#[test]
fn a_single_item_still_ends_at_the_document() {
    // THE CONTROL FOR THE LADDER ITSELF. With no descendant there is no
    // descendant column, and #1512's answer stands: the item ends and the line
    // reparses at document level.
    assert_eq!(
        both_paths("- x\n  %%% x\n # h\n"),
        "<ul>\n  <li>x</li>\n</ul>\n<p># h</p>"
    );
}

#[test]
fn the_outermost_frame_still_absorbs_the_line() {
    // THE CONTROL FOR THE `in_item_body` GATE. Firing the descendant band in
    // the DOCUMENT-level frame too ends every item including the outermost, and
    // the line leaves the list entirely as a top-level `<p># h</p>`.
    let output = both_paths("- - - x\n      %%% x\n # h\n");
    assert!(output.starts_with("<ul>"), "{output}");
    assert!(!output.contains("<p># h</p>"), "{output}");
}

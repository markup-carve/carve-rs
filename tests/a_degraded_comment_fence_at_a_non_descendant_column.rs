//! A DEGRADED COMMENT FENCE IS PLACED BY ITS COLUMN, NOT BY A MEMBERSHIP TEST
//! (markup-carve/carve-rs#1545).
//!
//! The collector armed the degraded band on
//! `descendant_columns.contains(&indent)`, so a fence at a column that is not
//! exactly some listed descendant's armed nothing. Under `- - - x` the listed
//! columns are 2, 4 and 6; a fence at 5 fell through every arm and the item
//! ended in the wrong place. The CLOSED spelling of the same fence has used
//! `indent >= strip_cols` since markup-carve/carve-rs#1531, which is why the two
//! spellings answered one document two different ways - the exact defect #1531
//! set out to remove.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at carve
//! `8898a1a5`, spec main. Every expectation below was produced by running it on
//! the document.
//!
//! MEASURED. 14304 pairs - l/q ladders of depth two to four, both fence widths,
//! the fence at every column from 0 to the innermost, four follower kinds at
//! every column - each pair the same document in both spellings, compared
//! whitespace-SENSITIVELY against the spec.
//!
//! Restricted to the band the fence actually REACHES (written at or past the
//! outermost content column), 1760 pairs:
//!
//! | | closed wrong | degraded wrong | spellings disagree |
//! | --- | --- | --- | --- |
//! | before | 50 | 66 | 18 |
//! | after | 50 | 50 | 0 |
//!
//! So the degraded half landed exactly on the closed half and the two spellings
//! now agree on every pair in the band. The 50 that remain are wrong in BOTH
//! spellings, which is a different defect and not a split.
//!
//! Over the whole grid: 64 fixed, 0 newly broken, and the 1649-document corpus
//! at carve `063656e7` is byte-identical.
//!
//! NOT IN SCOPE. A fence written BELOW every content column reaches no container
//! and arms nothing, which is correct and pinned here as a control. All 864
//! remaining degraded-only divergences have their fence in that band, where what
//! moves is the FOLLOWER - the below-column-marker family of
//! markup-carve/carve-rs#1514 and the column-0 family of #1529.

use carve::{to_html, to_html_with_options, Options};

/// The #908 guard. The collector this changes is the one both paths run since
/// markup-carve/carve-rs#1490, but the assertion stays: it is what caught the
/// two paths splitting while #1540 was being written.
fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

/// `# h` as the OUTERMOST item's text, the three-deep ladder having ended.
const OUTER_TEXT: &str = "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          \
     <li>x</li>\n        </ul>\n      </li>\n    </ul>\n    # h\n  </li>\n</ul>";

/// The reported document. Column 5 is neither the innermost frame's own content
/// column (6) nor a listed descendant's (2, 4).
#[test]
fn the_reported_document_ends_the_same_items_as_the_closed_spelling() {
    assert_eq!(both_paths("- - - x\n     %%% x\n # h\n"), OUTER_TEXT);
}

/// THE PAIR THE TICKET IS ABOUT. The closed spelling was already right; the two
/// must now be one answer.
#[test]
fn the_two_spellings_answer_the_same_document_alike() {
    let degraded = both_paths("- - - x\n     %%% x\n # h\n");
    let closed = both_paths("- - - x\n     %%% x\n     %%%\n # h\n");
    assert_eq!(degraded, closed, "the two spellings disagree");
    assert_eq!(closed, OUTER_TEXT);
}

/// A LISTED DESCENDANT'S COLUMN still answers the way #1518 made it, which is
/// what says the column test is a widening rather than a replacement.
#[test]
fn a_listed_descendant_column_is_unchanged() {
    assert_eq!(both_paths("- - - x\n    %%% x\n # h\n"), OUTER_TEXT);
}

/// THE INNERMOST FRAME'S OWN COLUMN is the arm above this one and ends nothing
/// of its own; the ladder still ends because the line reached none of it.
#[test]
fn the_frames_own_column_is_unchanged() {
    assert_eq!(both_paths("- - - x\n      %%% x\n # h\n"), OUTER_TEXT);
}

/// A FENCE BELOW EVERY CONTENT COLUMN reached no container, so it arms nothing
/// and the follower folds where it was already going. THE CONTROL for the `>`
/// rather than `>=`, and the band the remaining divergences live in.
#[test]
fn a_fence_below_every_column_ends_nothing() {
    assert_eq!(
        both_paths("- - - x\n %%% x\n # h\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>x\n            \
         # h\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

/// A FOLLOWER THAT REACHES the outermost item is that item's content, so it is a
/// heading there rather than text. The carried flag answers this, not the column.
#[test]
fn a_follower_that_reaches_the_outer_item_is_a_heading_there() {
    assert_eq!(
        both_paths("- - - x\n     %%% x\n   # h\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>x</li>\n        \
         </ul>\n      </li>\n    </ul>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

/// A PLAIN follower answers the same way - the kind of the line is not what
/// decides it.
#[test]
fn a_text_follower_answers_the_same_way() {
    assert_eq!(
        both_paths("- - - x\n     %%% x\n b\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>x</li>\n        \
         </ul>\n      </li>\n    </ul>\n    b\n  </li>\n</ul>"
    );
}

/// A QUOTE HOST answers the same way, and it moved with this change too.
#[test]
fn a_quote_host_answers_the_same_way() {
    assert_eq!(
        both_paths("> - - x\n>      %%% x\n>  # h\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <ul>\n        <li>x</li>\n      </ul>\n      \
         # h\n    </li>\n  </ul>\n</blockquote>"
    );
}

/// OUT OF SCOPE, PINNED SO IT CANNOT MOVE SILENTLY. At depth two with the fence
/// at column 3 the spec folds `# h` into the INNER item; this engine puts a
/// heading in the outer one. Both spellings answer it that way, before and
/// after, so it is the shared residue rather than a split - one of the 50.
#[test]
fn the_shared_residue_does_not_move() {
    let degraded = both_paths("- - x\n   %%% x\n # h\n");
    let closed = both_paths("- - x\n   %%% x\n   %%%\n # h\n");
    assert_eq!(
        degraded, closed,
        "the residue must stay SHARED, not become a split"
    );
    assert_eq!(
        degraded,
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

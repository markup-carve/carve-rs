//! A definition list that took an authored base keeps it across a blank line.
//!
//! PART 9 §24 C3's authored-base clause (carve#1729) puts a recognized block
//! opener at or past a body's minimum content column in that body and makes
//! its authored visual column the local `block_base`, so that "the block's
//! payload, continuations and closer are measured relative to that base". A
//! definition body's continuations include the blank-separated indented block
//! of PART 9 §17 FORM A.
//!
//! The engine ended the rebased run at the blank line instead. The definition
//! above the blank had moved to the base and the block below it had not, so
//! every column read alike: the block left the description whether it was
//! written at the description's column, below it, or at the list's own
//! (carve-rs#1419). Spec corpus category 419 pins all three columns; two of
//! its three documents failed here.
//!
//! THE LADDER HAS ONE BOUNDARY NOW, not two. carve#1729 spelled the clause per
//! container, and this file pinned what that spelling produced: a band below
//! the description's column where the opener folded into the paragraph above it
//! as literal text, and a list item that answered the same geometry differently
//! from a footnote body. carve#1781 replaced the three spellings with one - THE
//! BASE BELONGS TO THE INNERMOST OPEN CONTAINER - and carve#1791 added the list
//! item to it. Below the description's column the description simply ENDS, and
//! the surviving container is the body the run was written in, where a
//! recognized opener is still an opener (carve-rs#1430). The three tests that
//! asserted the superseded reading now assert this one, and say which.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

/// The footnote body's minimum column is 2 and the list is authored at 3, so
/// the description's own column is 6. Everything below varies only the column
/// the quote is written at.
fn footnote_with_quote_at(column: usize) -> String {
    format!(
        "[^n]: intro\n\n   :: term\n   :  definition\n\n{}> quote\n\nsee[^n]\n",
        " ".repeat(column)
    )
}

#[test]
fn a_block_at_the_description_column_opens_inside_the_description() {
    let output = html(&footnote_with_quote_at(6));
    assert!(
        output.contains(
            "<dd>\n          <p>definition</p>\n          \
             <blockquote><p>quote</p></blockquote>\n        </dd>"
        ),
        "{output}"
    );
}

#[test]
fn one_column_short_of_the_description_is_the_bodys_own_block() {
    // The other side of the upper boundary. Below the description's column the
    // description ENDS, and the surviving container is the footnote body - where
    // the quote is still a quote. This used to read as literal text: the run
    // carried the line along and dedented it by the run's base alone, which put
    // it between the two columns, too shallow to be the description's content
    // and no longer at the body's minimum (carve-rs#1430).
    let output = html(&footnote_with_quote_at(5));
    assert!(output.contains("<dd>definition</dd>"), "{output}");
    assert!(
        output.contains("</dl>\n      <blockquote><p>quote</p></blockquote>"),
        "{output}"
    );
    assert!(!output.contains("&gt; quote"), "{output}");
}

#[test]
fn at_or_below_the_lists_own_column_the_block_leaves_the_list() {
    // The lower boundary, on both sides of the list's authored column. The
    // quote is the footnote body's own block, a sibling of the list.
    for column in [2, 3] {
        let output = html(&footnote_with_quote_at(column));
        assert!(output.contains("<dd>definition</dd>"), "{column}: {output}");
        assert!(
            output.contains("</dl>\n      <blockquote><p>quote</p></blockquote>"),
            "{column}: {output}"
        );
    }
}

#[test]
fn the_boundary_moves_with_the_description_not_with_the_list() {
    // The list is authored AT the footnote body's minimum column, so it takes
    // no authored base - but the description it opens still has a content
    // column, and that is the boundary. It is 5 here against 6 above, and the
    // ladder shifts with it by exactly one.
    //
    // The whole ladder used to read BESIDE, because with no rebased run there
    // was nothing to register the description as a container: a block at the
    // description's own column was measured against the footnote body and
    // lifted out of the description it was written into (carve-rs#1430).
    for column in 2..=8 {
        let output = html(&format!(
            "[^n]: intro\n\n  :: term\n  :  definition\n\n{}> quote\n\nsee[^n]\n",
            " ".repeat(column)
        ));
        if column >= 5 {
            assert!(
                output.contains(
                    "<dd>\n          <p>definition</p>\n          \
                     <blockquote><p>quote</p></blockquote>\n        </dd>"
                ),
                "{column}: {output}"
            );
        } else {
            assert!(output.contains("<dd>definition</dd>"), "{column}: {output}");
            assert!(
                output.contains("</dl>\n      <blockquote><p>quote</p></blockquote>"),
                "{column}: {output}"
            );
        }
    }
}

#[test]
fn the_blank_line_makes_no_difference_at_any_column() {
    // The defect stated as its own invariant. A blank line loosens the
    // description; it does not move ownership, so the two spellings agree at
    // every column on both sides of both boundaries.
    for column in 2..=9 {
        let indent = " ".repeat(column);
        let blank = html(&format!(
            "[^n]: intro\n\n   :: term\n   :  definition\n\n{indent}> quote\n\nsee[^n]\n"
        ));
        let tight = html(&format!(
            "[^n]: intro\n\n   :: term\n   :  definition\n{indent}> quote\n\nsee[^n]\n"
        ));
        let opened = |html: &str| html.contains("<blockquote>");
        let inside = |html: &str| {
            html.find("<blockquote>")
                .zip(html.find("</dl>"))
                .is_some_and(|(quote, list)| quote < list)
        };
        assert_eq!(
            (opened(&blank), inside(&blank)),
            (opened(&tight), inside(&tight)),
            "column {column}:\nblank:\n{blank}\ntight:\n{tight}"
        );
    }
}

#[test]
fn the_rule_is_not_about_quotes() {
    // Every recognized opener takes the same base. A heading closes itself, so
    // it cannot be explained by a quote swallowing the line.
    let output = html("[^n]: intro\n\n   :: term\n   :  definition\n\n      # h\n\nsee[^n]\n");
    assert!(
        output.contains(
            "<dd>\n          <p>definition</p>\n          \
             <h1 id=\"h\">h</h1>\n        </dd>"
        ),
        "{output}"
    );
}

#[test]
fn a_list_item_answers_the_same_ladder() {
    // A LIST ITEM IS A CONTAINER THE RULE REACHES (carve#1791). This test used
    // to assert the opposite - that PART 9 §24 C3 named "a definition body's
    // column 3 or a footnote body's column 2" and a list item was outside the
    // clause, so the identical document inside an item left the quote beside
    // the list at EVERY payload column. carve#1781 replaced the per-container
    // spellings with one rule and carve#1791 added the item to it, so the
    // ladder is now the footnote body's ladder with the outer container
    // swapped: the boundary is the description's content column, 6 here, in
    // both.
    //
    // The nesting question the old comment was really guarding - a block at a
    // nested LIST MARKER's content column, which carve-js#1508 got wrong and
    // #1520 unpicked - is answered elsewhere and did not move: a marker line
    // registers its own content column and returns before this rebase can see
    // it. `a_nested_list_marker_keeps_the_block_in_its_item` below pins it.
    for column in 2..=9 {
        let output = html(&format!(
            "- item\n\n   :: term\n   :  definition\n\n{}> quote\n",
            " ".repeat(column)
        ));
        let expected = if column >= 6 {
            "<ul>\n  <li>item\n    <dl>\n      <dt>term</dt>\n      \
             <dd>\n        <p>definition</p>\n        \
             <blockquote><p>quote</p></blockquote>\n      </dd>\n    \
             </dl>\n  </li>\n</ul>"
        } else {
            "<ul>\n  <li>item\n    <dl>\n      <dt>term</dt>\n      \
             <dd>definition</dd>\n    </dl>\n    \
             <blockquote><p>quote</p></blockquote>\n  </li>\n</ul>"
        };
        assert_eq!(output, expected, "column {column}");
    }
}

#[test]
fn a_nested_list_marker_keeps_the_block_in_its_item() {
    // The shape the old list-item test was guarding, kept as its own claim.
    // A block at a nested item's content column belongs to that item, not to
    // the body the marker was written in - the marker is the innermost open
    // container there. carve#1791 corpus
    // `423-one-authored-base-rule-reaches-a-definition-nested-in-a-list-item-2`;
    // this engine already read it that way, and this pins it against the
    // description registration added for carve-rs#1430.
    assert_eq!(
        html("[^n]: intro\n\n  - item\n\n    > quote\n\nsee[^n]\n"),
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n\
         <section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    \
         <li id=\"fn1\">\n      <p>intro</p>\n      <ul>\n        <li>item\n          \
         <blockquote><p>quote</p></blockquote>\n        </li>\n      </ul>\n      \
         <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    \
         </li>\n  </ol>\n</section>"
    );
}

#[test]
fn a_nested_definition_writes_back_without_changing_ownership() {
    let source = "- intro\n\n   :: term\n   :  definition\n\n      > quote\n";
    let formatted = carve::to_carve(source);

    assert_eq!(
        formatted,
        "- intro\n  :: term\n  : definition\n\n    > quote\n"
    );
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

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
fn one_column_short_of_the_description_is_still_lazy_text() {
    // The other side of the upper boundary. Below the description's column the
    // body ends, and a line that is not the enclosing body's own block folds
    // into the paragraph above it as literal text - the `>` never opens.
    let output = html(&footnote_with_quote_at(5));
    assert!(output.contains("<dd>definition</dd>"), "{output}");
    assert!(output.contains("<p>&gt; quote"), "{output}");
    assert!(!output.contains("<blockquote>"), "{output}");
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
fn a_list_at_the_bodys_own_minimum_column_is_unchanged() {
    // The list is authored AT the footnote body's minimum column, so it takes
    // no authored base and there is no rebased run for a blank to end. The
    // block leaves the list at every column, which is what corpus 419's first
    // document already read here.
    for column in 2..=8 {
        let output = html(&format!(
            "[^n]: intro\n\n  :: term\n  :  definition\n\n{}> quote\n\nsee[^n]\n",
            " ".repeat(column)
        ));
        assert!(output.contains("<dd>definition</dd>"), "{column}: {output}");
        assert!(
            output.contains("</dl>\n      <blockquote><p>quote</p></blockquote>"),
            "{column}: {output}"
        );
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
fn a_list_item_is_outside_the_clause_and_keeps_its_own_answer() {
    // THE CONTAINERS THE CLAUSE NAMES ARE THE ONLY ONES THAT MOVE. PART 9 §24
    // C3 names "a definition body's column 3 or a footnote body's column 2".
    // A list item is not one of them, and it legitimately reads the same
    // geometry differently: the identical list, written at the identical
    // column inside a list item instead of a footnote body, leaves the block
    // beside the list at EVERY payload column - where a footnote body folds it
    // in as lazy text one and two columns short of the description and opens
    // it inside the description at the description's own column.
    //
    // Applying the clause to every container instead of the named two trades
    // this ticket's defect for that one. carve-js shipped that trade and took
    // two further tickets to unpick it (markup-carve/carve-js#1508, undone in
    // #1520). The rendering asserted below is what carve-js reads at
    // markup-carve/carve-js@10a1698e, whose agreement with the executable
    // oracle over this ladder is complete.
    //
    // WHAT THIS TEST IS AND IS NOT. It asserts the WHOLE document at every
    // column, so any change that moves list-item placement fails it. It is not
    // a tripwire on `include_sublists`: flipping that flag at both list-item
    // call sites was measured and moves no cell of this ladder, because a
    // marker line registers its content column and returns before the rebase
    // this ticket changes can see it. That insulation is why the fix could not
    // have made carve-js's trade here - but it is structural rather than
    // stated, so the answer is pinned by value.
    for column in 2..=9 {
        let output = html(&format!(
            "- item\n\n   :: term\n   :  definition\n\n{}> quote\n",
            " ".repeat(column)
        ));
        assert_eq!(
            output,
            "<ul>\n  <li>item\n    <dl>\n      <dt>term</dt>\n      \
             <dd>definition</dd>\n    </dl>\n    \
             <blockquote><p>quote</p></blockquote>\n  </li>\n</ul>",
            "column {column}"
        );
    }
}

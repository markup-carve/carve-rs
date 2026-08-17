//! A `+` continuation row extends a CELL, so the block an unclosed inline
//! verbatim run reaches the end of is that whole cell, continuation included
//! (markup-carve/carve#1293 part 1).
//!
//! carve-rs published `a <code>b</code> c<code></code>` for the shape below.
//! The deciding evidence is not that two other engines say otherwise - it is
//! the EMPTY `<code></code>`, which no clause in this language produces. It was
//! the mechanism showing through: the run was closed at the row's pipe and a
//! fresh one opened for the continuation, because each fragment was parsed on
//! its own and the results concatenated.
//!
//! What the rules already said points the same way. An unclosed inline verbatim
//! run renders to the end of the BLOCK (`edge-cases.md`, the clause
//! markup-carve/carve#1282 was settled on), and markup-carve/carve#1284
//! established that a row cuts into cells BEFORE inline parsing. A `+`
//! continuation extends the cell, so the block is that cell.
//!
//! TWO CONSEQUENCES FOLLOW AND ARE PINNED HERE TOO. The cell's content is
//! assembled before it is parsed, so a construct of any kind may span the row
//! boundary; and a `|` on the continuation row that sits INSIDE the open run is
//! that run's content rather than a cell separator, which is what stops the
//! text after it from being dropped for want of a column to join.
//!
//! The escaped closing pipe (part 2 of the same ticket) is NOT in scope here.

use carve::ast::{BlockNode, InlineNode};
use carve::to_html;

fn table_html(rows: &str) -> String {
    format!("<table>\n  <tbody>\n{rows}\n  </tbody>\n</table>")
}

fn first_row_cells(source: &str) -> Vec<carve::ast::TableCell> {
    let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("the fixture did not parse as a table");
    };
    table.rows[0].cells.clone()
}

/// The shape the ticket was filed on.
#[test]
fn an_open_run_closes_on_the_continuation_row() {
    assert_eq!(
        to_html("| a `b |\n+ c` |\n"),
        table_html("    <tr><td>a <code>b c</code></td></tr>")
    );
}

/// The artifact that decided it, asserted as its own fact: whatever else
/// changes, an empty verbatim span is not an answer this language can give.
#[test]
fn no_empty_code_span_is_published() {
    let html = to_html("| a `b |\n+ c` |\n");
    assert!(
        !html.contains("<code></code>"),
        "an empty code span is a mechanism showing through: {html}"
    );
}

/// A `|` on the continuation row that sits INSIDE the open run is the run's
/// content, exactly as it is on the row that opened it - so it does not cut a
/// cell, and the text after it is not dropped.
///
/// This is the case that separates "the run spans the continuation" from a fix
/// that only seeds the first column: splitting the continuation with a fresh
/// scanner cuts at that pipe, and every segment past the first then has no
/// column to join.
#[test]
fn a_pipe_inside_the_open_run_is_content_on_the_continuation_too() {
    assert_eq!(
        to_html("| a `b |\n+ c | d` |\n"),
        table_html("    <tr><td>a <code>b c | d</code></td></tr>")
    );
}

/// AND THE SEED BELONGS TO ONE COLUMN. The run was open in the row's LAST cell,
/// and a continuation joins PER COLUMN, so the columns before it are cut
/// normally and the pipe that ends them still separates.
///
/// Seeding the whole continuation line instead swallows that separator, puts
/// the continuation in the wrong cell, and leaves the run's own cell with the
/// empty `<code></code>` this ruling rejects - the same artifact reached from
/// the other side.
#[test]
fn the_open_run_seeds_only_the_column_it_was_open_in() {
    assert_eq!(
        to_html("| x | a `b |\n+ y | c` |\n"),
        table_html("    <tr><td>x y</td><td>a <code>b c</code></td></tr>")
    );
}

/// The two halves composed, which is the only shape that pins the RESEED
/// rather than the line-start seed.
///
/// `the_open_run_seeds_only_the_column_it_was_open_in` passes with no reseed at
/// all: its continuation holds one pipe and that pipe separates either way. The
/// reseed only decides when the seeded column's own content carries a pipe -
/// here the run is open in the SECOND column and the continuation's second
/// column holds `c | d`, so the first pipe separates and the second is content.
///
/// Dropping the reseed splits at both, and the third segment is dropped for
/// want of a column: `| d` disappears and the run's cell is left short. That is
/// the content loss the executable spec's `splitRow(line, openRun, openRunAt)`
/// was given its second parameter for.
#[test]
fn the_reseed_lets_a_later_column_keep_its_own_pipe() {
    assert_eq!(
        to_html("| x | a `b |\n+ y | c | d` |\n"),
        table_html("    <tr><td>x y</td><td>a <code>b c | d</code></td></tr>")
    );
}

/// The run stays open across as many continuation rows as it takes.
#[test]
fn an_open_run_reaches_the_last_continuation_row() {
    assert_eq!(
        to_html("| a `b |\n+ c |\n+ d` |\n"),
        table_html("    <tr><td>a <code>b c d</code></td></tr>")
    );
}

/// The cell is assembled before it is parsed, so the run is not the only
/// construct that spans the boundary. Emphasis is the same fact seen through a
/// different delimiter, and carve-js publishes the same tree.
#[test]
fn emphasis_spans_the_continuation_too() {
    assert_eq!(
        to_html("| a *b |\n+ c* |\n"),
        table_html("    <tr><td>a <strong>b c</strong></td></tr>")
    );
}

/// CONTROL: a run that CLOSES on the row is untouched. The continuation still
/// joins as ordinary text after it.
#[test]
fn a_closed_run_still_ends_where_it_was_written() {
    assert_eq!(
        to_html("| a `b c` |\n+ d |\n"),
        table_html("    <tr><td>a <code>b c</code> d</td></tr>")
    );
}

/// CONTROL: a continuation with no verbatim run anywhere joins per column with
/// a single space, as it always did.
#[test]
fn an_ordinary_continuation_joins_per_column() {
    assert_eq!(
        to_html("| a | b |\n+ c | d |\n"),
        table_html("    <tr><td>a c</td><td>b d</td></tr>")
    );
}

/// CONTROL: an EMPTY continuation cell contributes nothing - not even the
/// joining space - which is what lets a continuation address one column of a
/// wide row.
#[test]
fn an_empty_continuation_cell_contributes_nothing() {
    assert_eq!(
        to_html("| a | b |\n+  | d |\n"),
        table_html("    <tr><td>a</td><td>b d</td></tr>")
    );
}

/// CONTROL: the ROW's own closing pipe still closes the row with a run open
/// (markup-carve/carve#1284). Nothing here widens what a row is.
#[test]
fn the_rows_closing_pipe_still_closes_the_row() {
    assert_eq!(
        to_html("| a `b | c |\n"),
        table_html("    <tr><td>a <code>b | c</code></td></tr>")
    );
}

/// The merged value is ONE text node in `parse()` itself, and it carries no
/// span: the two halves are separated by a delimiter and a newline the value
/// does not contain, so a span across them would not select its own text.
/// Absent beats wrong (PART 12 section 4).
#[test]
fn a_value_joined_across_the_gap_carries_no_span() {
    let cells = first_row_cells("| x | A long description |\n+     | that continues     |\n");
    match &cells[1].children[0] {
        InlineNode::Text(t) => {
            assert_eq!(t.value, "A long description that continues");
            assert!(
                t.pos.is_none(),
                "a joined value cannot be placed: {:?}",
                t.pos
            );
        }
        other => panic!("expected one merged text node, got {other:?}"),
    }
    assert_eq!(cells[1].children.len(), 1, "the cell holds one run");
}

/// A node that lies WHOLLY INSIDE one fragment keeps its own position, and the
/// fragment it came from decides which line that is. Losing this is the cost of
/// assembling the cell if the anchors are not carried with the fragments.
#[test]
fn a_node_inside_the_continuation_keeps_its_own_position() {
    let source = "| a | b |\n+ c | /d/ |\n";
    let codepoints: Vec<char> = source.chars().collect();
    let cells = first_row_cells(source);
    let InlineNode::Emphasis(em) = cells[1]
        .children
        .iter()
        .find(|n| matches!(n, InlineNode::Emphasis(_)))
        .expect("the continuation's emphasis is a node")
    else {
        unreachable!()
    };
    let pos = em
        .pos
        .expect("a node inside one fragment carries a position");
    assert_eq!(pos.start_line, 2, "the anchor came from the row, not the +");
    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert_eq!(slice, "/d/", "the span points somewhere else");
}

/// And the FIRST fragment keeps its own position too - assembling the cell must
/// not cost the row line its anchors.
#[test]
fn a_node_on_the_row_line_keeps_its_own_position() {
    let source = "| a | /b/ |\n+ c | d |\n";
    let codepoints: Vec<char> = source.chars().collect();
    let cells = first_row_cells(source);
    let InlineNode::Emphasis(em) = &cells[1].children[0] else {
        panic!("expected the row's emphasis first");
    };
    let pos = em.pos.expect("the row line's node carries a position");
    assert_eq!(pos.start_line, 1);
    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert_eq!(slice, "/b/");
}

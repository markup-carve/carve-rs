//! A table cell's padding slots take U+0020 only. A tab there is not padding.
//!
//! Spec PART 7, MARKER SEPARATORS AND PADDING SLOTS: a tab is syntax ONLY in a
//! line's LEADING INDENTATION run. Every table-cell padding slot sits after the
//! row's opening `|`, so every one of them is inline and takes `space`. Five
//! productions in `resources/grammar.ebnf` were narrowed by
//! markup-carve/carve#910:
//!
//! ```text
//! delimiter_cell = {space}, [':'], '-', {'-'}, [':'], {space} ;
//! header_cell    = '=', [alignment_marker], {space}, cell_content, {space} ;
//! data_cell      = [cell_attributes], [alignment_marker], {space}, cell_content, {space} ;
//! rowspan_marker = {space}, '^', {space} ;
//! colspan_marker = {space}, '<', {space} ;
//! ```
//!
//! A tab in a padding slot is NOT a rejection. It stops being padding and
//! becomes ordinary cell CONTENT, so it stays exactly where it was written. At
//! `delimiter_cell` the failure is structural instead of textual: the cell is no
//! longer a delimiter cell, so the line is not a delimiter row - no header is
//! promoted, no alignment is assigned, and the `---` run is inline content that
//! smart typography renders as an em dash.
//!
//! Every expectation below is the corresponding document of the spec corpus
//! category `256-table-cell-padding-must-be-a-space`, byte-for-byte. That
//! category is not reachable from this repository yet: the `tests/spec`
//! submodule is pinned at cf5c03a and the category landed after it, and
//! `tests/corpus.rs` rejects an IMPLEMENTED entry with no corpus pair behind it.
//! These assertions therefore stand in for the corpus until the pin moves, which
//! is what keeps the fix from being invisible (markup-carve/carve-rs#730, the
//! class catalogued in markup-carve/carve#755).
//!
//! Pre-fix this engine reproduced 5 of the 21 documents; the 16 it failed are
//! exactly the ones carrying a tab.
//!
//! Cardinality is deliberately untouched: `{space}` is a RUN, so `|=  i |` is
//! still padded, not content. markup-carve/carve#912 settles cardinality for
//! four OTHER productions and does not reach these.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

// ---------------------------------------------------------------------------
// data_cell - the leading slot
// ---------------------------------------------------------------------------

#[test]
fn a_tab_opening_a_data_cell_is_content() {
    // corpus 256-1. The tab is the FIRST character after the pipe.
    assert_eq!(
        html("|\ta |\tb |\n"),
        "<table>\n  <tbody>\n    <tr><td>\ta</td><td>\tb</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_space_then_a_tab_opening_a_data_cell_is_content() {
    // corpus 256-2, and TRAP 1: the slot is a RUN, not a first character. A fix
    // written as "the first character must be a space" passes the tab-first case
    // above and still lets this one through. That exact defect was found three
    // times in one day, in three languages. The leading space IS consumed as
    // padding; the tab that follows it ends the run and is content.
    assert_eq!(
        html("| \ta | \tb |\n"),
        "<table>\n  <tbody>\n    <tr><td>\ta</td><td>\tb</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_then_a_space_opening_a_data_cell_keeps_both() {
    // corpus 256-3. The other mixed run: the tab ends the padding immediately,
    // so the space AFTER it is content too and survives into the cell.
    assert_eq!(
        html("|\t a |\t b |\n"),
        "<table>\n  <tbody>\n    <tr><td>\t a</td><td>\t b</td></tr>\n  </tbody>\n</table>"
    );
}

// ---------------------------------------------------------------------------
// data_cell - the trailing slot
// ---------------------------------------------------------------------------

#[test]
fn a_tab_closing_a_data_cell_is_content() {
    // corpus 256-4, and TRAP 2: each end reverts independently. The leading and
    // trailing slots are two separate edits, and a fixture carrying a tab at
    // BOTH ends cannot tell a half-fix from a whole one - so both ends are
    // pinned on their own here.
    assert_eq!(
        html("| a\t| b\t|\n"),
        "<table>\n  <tbody>\n    <tr><td>a\t</td><td>b\t</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_space_then_a_tab_closing_a_data_cell_is_content() {
    // corpus 256-5. The trailing slot is scanned from the pipe backwards, so the
    // mixed run that catches a first-character test at this end is `<SP><TAB>`
    // rather than the leading end's - the tab is what the scan meets first.
    assert_eq!(
        html("| a \t| b \t|\n"),
        "<table>\n  <tbody>\n    <tr><td>a \t</td><td>b \t</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_then_a_space_closing_a_data_cell_keeps_the_tab() {
    // NOT a corpus document, and it should be. The category carries both mixed
    // runs at the LEADING end (256-2, 256-3) but only `<SP><TAB>` at the
    // trailing one (256-5), and `<SP><TAB>` is the run a trailing
    // first-character test happens to get right: the tab is last, so the test
    // never fires. `<TAB><SP>` is the one that catches it - the trailing space
    // IS padding, and the tab before it ends the run and stays.
    //
    // Found by mutation: narrowing the helper to a first-character test killed
    // only the two leading mixed-run assertions, leaving the trailing end's run
    // property unpinned by this file and by the corpus both.
    assert_eq!(
        html("| a\t | b\t |\n"),
        "<table>\n  <tbody>\n    <tr><td>a\t</td><td>b\t</td></tr>\n  </tbody>\n</table>"
    );
}

// ---------------------------------------------------------------------------
// header_cell - both ends, both mixed runs
// ---------------------------------------------------------------------------

#[test]
fn a_tab_opening_a_header_cell_is_content() {
    // corpus 256-6. The `=` is still glued to the pipe, so the cell is a header
    // cell; only its padding slot changes. The row still promotes to a `<thead>`.
    assert_eq!(
        html("|=\th |=\ti |\n| 1 | 2 |\n"),
        "<table>\n  <thead><tr><th>\th</th><th>\ti</th></tr></thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_then_a_space_opening_a_header_cell_keeps_both() {
    // corpus 256-7.
    assert_eq!(
        html("|=\t h |=\t i |\n| 1 | 2 |\n"),
        "<table>\n  <thead><tr><th>\t h</th><th>\t i</th></tr></thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_closing_a_header_cell_is_content() {
    // corpus 256-8. The header cell's trailing slot is a THIRD site, reached
    // through the `=` branch rather than the plain one.
    assert_eq!(
        html("|= h\t|= i\t|\n| 1 | 2 |\n"),
        "<table>\n  <thead><tr><th>h\t</th><th>i\t</th></tr></thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_space_then_a_tab_closing_a_header_cell_is_content() {
    // corpus 256-9.
    assert_eq!(
        html("|= h \t|= i \t|\n| 1 | 2 |\n"),
        "<table>\n  <thead><tr><th>h \t</th><th>i \t</th></tr></thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_then_a_space_closing_a_header_cell_keeps_the_tab() {
    // The header cell's copy of the gap above; it is a separate code path from
    // the plain cell's, so a half-fix at this end shows up here and nowhere
    // else.
    assert_eq!(
        html("|= a\t |= b\t |\n| 1 | 2 |\n"),
        "<table>\n  <thead><tr><th>a\t</th><th>b\t</th></tr></thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

// ---------------------------------------------------------------------------
// delimiter_cell - where the failure is structural
// ---------------------------------------------------------------------------

#[test]
fn a_tab_opening_a_delimiter_cell_unmakes_the_delimiter_row() {
    // corpus 256-10. This is the one production whose narrowing is visible in
    // the TABLE'S SHAPE rather than in a cell's text: the cell is no longer a
    // delimiter cell, so line 2 is not a delimiter row. Nothing is promoted to a
    // header, no alignment is assigned, and `---` becomes ordinary content that
    // smart typography renders as an em dash.
    assert_eq!(
        html("| a | b |\n|\t--- |\t--- |\n| 1 | 2 |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>\t—</td><td>\t—</td></tr>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_space_then_a_tab_opening_a_delimiter_cell_unmakes_it_too() {
    // corpus 256-11. The mixed-run form of the structural failure.
    assert_eq!(
        html("| a | b |\n| \t--- | \t--- |\n| 1 | 2 |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>\t—</td><td>\t—</td></tr>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_closing_a_delimiter_cell_unmakes_it_too() {
    // corpus 256-12. The delimiter cell's trailing end, which reverts
    // independently of its leading one exactly as the data cell's does.
    assert_eq!(
        html("| a | b |\n| ---\t| ---\t|\n| 1 | 2 |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>—\t</td><td>—\t</td></tr>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_then_a_space_closing_a_delimiter_cell_unmakes_it_too() {
    // The delimiter cell's copy of the same gap, and the one where getting it
    // wrong is loudest: a trailing first-character test would trim the tab away,
    // leave `---`, and promote a header row that the rule says is not there.
    assert_eq!(
        html("| a | b |\n| ---\t | ---\t |\n| 1 | 2 |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>—\t</td><td>—\t</td></tr>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>"
    );
}

// ---------------------------------------------------------------------------
// data_cell - the slot AFTER a per-cell alignment marker
// ---------------------------------------------------------------------------

#[test]
fn a_tab_after_a_per_cell_alignment_marker_is_content() {
    // NOT a corpus document either. `data_cell` reads
    // `[cell_attributes], [alignment_marker], {space}, cell_content, {space}`,
    // so a cell carrying a GLUED `<` / `>` / `~` marker has its padding slot
    // AFTER that marker - a fourth trim site, on its own branch, that no
    // document in the category exercises.
    //
    // Found by mutation: reverting that branch alone left all 24 other
    // assertions green. The cell still aligns, because the marker is glued to
    // the pipe and the narrowing does not touch it; only the padding after it
    // changes, so the tab survives into the cell.
    assert_eq!(
        html("|<\tx |>\ty |\n"),
        "<table>\n  <tbody>\n    <tr><td style=\"text-align: left;\">\tx</td><td style=\"text-align: right;\">\ty</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_space_after_a_per_cell_alignment_marker_is_padding() {
    // CONTROL for the case above: with a space the slot is padding as always,
    // so the alignment and the bare content both survive. This is what fails if
    // the marker branch is narrowed to something stricter than a space run.
    assert_eq!(
        html("|< x |> y |\n"),
        "<table>\n  <tbody>\n    <tr><td style=\"text-align: left;\">x</td><td style=\"text-align: right;\">y</td></tr>\n  </tbody>\n</table>"
    );
}

// ---------------------------------------------------------------------------
// rowspan_marker / colspan_marker - forced, not optional
// ---------------------------------------------------------------------------

#[test]
fn a_tab_beside_a_rowspan_marker_makes_the_cell_ordinary_content() {
    // corpus 256-13, and TRAP 4: the span markers are FORCED to move with the
    // other three. A span is recognized by comparing the TRIMMED cell against
    // `^`, so narrowing the cell trim narrows the marker automatically - the
    // trimmed cell is now `<TAB>^`, which is not the marker, and the row above
    // gets no `rowspan`.
    assert_eq!(
        html("| a | b |\n|\t^ | c |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>\t^</td><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_beside_a_colspan_marker_makes_the_cell_ordinary_content() {
    // corpus 256-14. The colspan half of the same rule; the `<` is escaped on
    // the way out because it is content now.
    assert_eq!(
        html("| a | b |\n| c |\t< |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>c</td><td>\t&lt;</td></tr>\n  </tbody>\n</table>"
    );
}

// ---------------------------------------------------------------------------
// continuation_row - the SECOND spelling of data_cell
// ---------------------------------------------------------------------------

#[test]
fn a_tab_opening_a_continuation_cell_is_content() {
    // corpus 256-15, and TRAP 3: a continuation row's cells are `data_cell`s
    // too, and they are padded in a SECOND place in the code
    // (`apply_table_continuation`). Narrowing only the standard-row path leaves
    // the continuation path joining the tab away with nothing able to see it -
    // which is what happened in the spec oracle.
    //
    // The joiner between the two lines' text is a manufactured space, so the
    // cell reads `a` + `<SP>` + `<TAB>x`.
    assert_eq!(
        html("| a | b |\n+\tx | y |\n"),
        "<table>\n  <tbody>\n    <tr><td>a \tx</td><td>b y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_tab_closing_a_continuation_cell_is_content() {
    // corpus 256-16. The continuation path's trailing end, which is a separate
    // edit there as well.
    assert_eq!(
        html("| a | b |\n+ x\t| y\t|\n"),
        "<table>\n  <tbody>\n    <tr><td>a x\t</td><td>b y\t</td></tr>\n  </tbody>\n</table>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS - the space spellings, which must not move
// ---------------------------------------------------------------------------

#[test]
fn control_a_space_padded_continuation_cell_is_unchanged() {
    // corpus 256-17. CONTROL. The continuation row with ordinary space padding:
    // the padding is consumed and the two lines join with a single space.
    assert_eq!(
        html("| a | b |\n+ x | y |\n"),
        "<table>\n  <tbody>\n    <tr><td>a x</td><td>b y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_the_padding_slot_is_still_a_run_of_spaces() {
    // corpus 256-18. CONTROL, and the boundary of this change. PART 7 settles
    // WHICH character is a separator; it does not settle HOW MANY. `{space}` is
    // a run, so two spaces are still padding and `  b  ` is still `b`. A fix
    // that narrowed cardinality along with the terminal set would fail here.
    assert_eq!(
        html("|=h|=  i |\n|a|  b  |\n"),
        "<table>\n  <thead><tr><th>h</th><th>i</th></tr></thead>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_space_padded_delimiter_row_still_promotes_and_aligns() {
    // corpus 256-19. CONTROL. The delimiter row is the shape whose failure is
    // structural, so its working form is the one that proves the narrowing did
    // not simply break delimiter rows: the header promotes and the right-align
    // colon still reaches both the `<th>` and the `<td>` below it.
    assert_eq!(
        html("| a | b |\n| --- | ---: |\n| 1 | 2 |\n"),
        "<table>\n  <thead><tr><th>a</th><th style=\"text-align: right;\">b</th></tr></thead>\n  <tbody>\n    <tr><td>1</td><td style=\"text-align: right;\">2</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_space_padded_rowspan_marker_still_spans() {
    // corpus 256-20. CONTROL for the forced marker: with spaces the marker is
    // still the whole trimmed cell, so the row above gets its `rowspan`.
    assert_eq!(
        html("| a | b |\n| ^ | c |\n"),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\">a</td><td>b</td></tr>\n    <tr><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_space_padded_colspan_marker_still_spans() {
    // corpus 256-21. CONTROL, the colspan half.
    assert_eq!(
        html("| a | b |\n| c | < |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td colspan=\"2\">c</td></tr>\n  </tbody>\n</table>"
    );
}

//! What a line at a container's CONTENT COLUMN ends
//! (markup-carve/carve#1348 and markup-carve/carve#1350, corpus categories 349
//! and 350; reported against this engine as markup-carve/carve-rs#1091).
//!
//! ONE INVARIANT, stated once because this family has come back four times a
//! prefix at a time:
//!
//! > A container ends at a flush-left line exactly when no block in the stack
//! > leaves a paragraph open, and what a line at the container's content column
//! > does to that paragraph is decided by the BLOCK the line belongs to - never
//! > by how the line is spelled.
//!
//! PART 1 S4 asks what a container's last BLOCK is. Three constructs answered it
//! from the line instead, and each got a different half of the same rule wrong:
//!
//! - a TABLE CONTINUATION ROW reads as prose on its own, so a quote ending on
//!   one recorded an open paragraph while the SAME table ending on a standard
//!   row closed the quote. §5 T6 joins a continuation row onto the row above, so
//!   it is as much a part of the table as the row it appends to.
//! - a LINK OR FOOTNOTE DEFINITION at a definition body's content column is an
//!   INTERRUPTER by §10 I5, and it registers there. The `dd` reached neither
//!   half: the prepass tracked list item columns only, so the line was ordinary
//!   text and defined nothing.
//! - a COMMENT at that same column is on I5's list too, and it publishes
//!   nothing - so the block-level test could not see it.
//!
//! The three are one rule and one of them proves it: a collected definition is
//! replaced by `DEFINITION_PLACEHOLDER`, a comment line, before the fold is ever
//! asked. So `:  a` / `   [r]: /u` and `:  a` / `   %% c` are the same shape by
//! then, and one answer closes both.
//!
//! WHAT DELIBERATELY DOES NOT MOVE is at the bottom of this file. Every row
//! there is an intended survivor with a reason, not an untested corner.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

// ---------------------------------------------------------------------------
// A table is a table however its last row is spelled (markup-carve/carve#1348)
// ---------------------------------------------------------------------------

/// Corpus `349-3`, the row this engine was alone on and in the direction
/// opposite to the other two engines: it answered the BARE quote wrong and the
/// definition-WRAPPED one right, where carve-js and carve-php did the reverse.
#[test]
fn a_quote_ending_on_a_continuation_row_ends_at_the_flush_left_line() {
    assert_eq!(
        html("> | a |\n> + b |\ntail\n"),
        "<blockquote>\n  <table>\n    <tbody>\n      <tr><td>a b</td></tr>\n    \
         </tbody>\n  </table>\n</blockquote>\n<p>tail</p>"
    );
}

/// The control that makes the row above evidence. Changing ONLY the last row's
/// spelling must not move `tail`: one question answered two ways by a spelling
/// is a contradiction rather than a second reading.
#[test]
fn the_standard_row_spelling_of_the_same_quote_answers_alike() {
    let standard = html("> | a |\n> | b |\ntail\n");
    let continuation = html("> | a |\n> + b |\ntail\n");
    assert!(
        standard.ends_with("</blockquote>\n<p>tail</p>"),
        "{standard}"
    );
    assert!(
        continuation.ends_with("</blockquote>\n<p>tail</p>"),
        "{continuation}"
    );
}

/// Every container spelling, as a property rather than as the corpus's rows.
/// The list item and the definition body were already right; the point is that
/// they answer ALIKE, so a fix that reached only the quote would leave the rule
/// stated three times.
#[test]
fn every_container_ending_on_a_continuation_row_ends_at_the_flush_left_line() {
    for src in [
        "- | a |\n  + b |\ntail\n",
        "> | a |\n> + b |\ntail\n",
        ":: t\n:  | a |\n   + b |\ntail\n",
        ":: t\n:  > | a |\n   > + b |\ntail\n",
    ] {
        let out = html(src);
        assert!(
            out.ends_with("<p>tail</p>"),
            "the container swallowed the line it ends on:\n{src}\n{out}"
        );
        assert!(
            out.contains("<td>a b</td>"),
            "the continuation row did not join the row above:\n{src}\n{out}"
        );
    }
}

// ---------------------------------------------------------------------------
// A definition at a container's content column (markup-carve/carve#1350)
// ---------------------------------------------------------------------------

/// Corpus `350-5`. BOTH halves, because an engine can get `tail` right by
/// dropping the definition instead of by ending the paragraph - which is right
/// by accident and fails the moment the column moves. The collapsed form `[r][]`
/// is what asks the second half: a bare `[r]` is literal in Carve.
#[test]
fn a_link_definition_at_a_definition_bodys_content_column_ends_the_paragraph() {
    assert_eq!(
        html(":: t\n:  a\n   [r]: /u\ntail\n\n[r][]\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>a</dd>\n</dl>\n<p>tail</p>\n\
         <p><a href=\"/u\">r</a></p>"
    );
}

/// The other kind on I5's list. It moves with the link kind or the rule is
/// being read off a spelling again.
#[test]
fn a_footnote_definition_at_that_column_answers_the_same_way() {
    let out = html(":: t\n:  a\n   [^f]: n\ntail\n\nx[^f]\n");
    assert!(
        out.starts_with("<dl>\n  <dt>t</dt>\n  <dd>a</dd>\n</dl>\n<p>tail</p>"),
        "{out}"
    );
    assert!(
        out.contains("role=\"doc-endnotes\""),
        "the note did not register:\n{out}"
    );
}

/// Corpus `350-6`. §10 I5 lists a comment beside the two definition kinds, and
/// the `dd` must answer it the same way - which is also the only way the row
/// above can work, since a collected definition arrives here AS a comment.
#[test]
fn a_comment_at_that_column_ends_the_paragraph_too() {
    assert_eq!(
        html(":: t\n:  a\n   %% c\ntail\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>a</dd>\n</dl>\n<p>tail</p>"
    );
}

/// §17 L1a: an invisible construct has no visible effect. The `dd` chose its
/// tight form by counting NODES, so a trailing comment - which publishes
/// nothing - put it in the loose form, while the list twin, which filters the
/// same set before deciding, stayed tight. Found while making the row above
/// byte-exact; it is a defect on its own and reproduces with no `tail` in sight.
#[test]
fn a_comment_that_publishes_nothing_does_not_loosen_the_definition_it_sits_in() {
    assert_eq!(
        html(":: t\n:  a\n   %% c\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>a</dd>\n</dl>"
    );
    // The twin that was already right, and the reason the answer above is not a
    // preference: the two containers must not differ on a construct that
    // reaches no target.
    assert_eq!(html("- a\n  %% c\n"), "<ul>\n  <li>a</li>\n</ul>");
}

// ---------------------------------------------------------------------------
// Controls: rows that must NOT move
// ---------------------------------------------------------------------------

/// I5's OPPOSITE VALUE. With no table above it a `+ ...|` line is not a row at
/// all but ordinary prose, so it leaves a paragraph open and the flush-left line
/// folds (markup-carve/carve#1345). The rule has a parameter and a reading that
/// answers "row" everywhere gets this half wrong.
#[test]
fn a_continuation_row_with_no_table_above_it_is_prose() {
    assert_eq!(
        html("> a\n> + b |\ntail\n"),
        "<blockquote><p>a\n+ b |\ntail</p></blockquote>"
    );
    assert_eq!(
        html("- a\n  + b |\ntail\n"),
        "<ul>\n  <li>a\n+ b |\ntail</li>\n</ul>"
    );
}

/// The row loop does NOT take a continuation directly after the GFM delimiter
/// row - the separator skips the continuation scan and the table ends there. A
/// scan that answered "row" for it would put the container boundary somewhere
/// the table reader never put it, which is markup-carve/carve#1354 in this
/// engine's own code.
#[test]
fn a_continuation_after_the_delimiter_row_is_read_as_the_table_reader_reads_it() {
    let out = html("> | a |\n> |---|\n> + b |\ntail\n");
    assert!(
        out.contains("<p>+ b |\ntail</p>"),
        "the scan and the table reader disagree about where the table ends:\n{out}"
    );
}

/// I5's other two columns are unchanged. BELOW the content column the line is
/// lazy paragraph text and registers nothing - which is the half that matters,
/// since an engine that ends the container while dropping the definition is
/// right for the wrong reason.
#[test]
fn a_definition_below_the_content_column_registers_nothing() {
    assert!(
        !html(":: t\n:  a\n  [r]: /u\ntail\n\n[r][]\n").contains("href=\"/u\""),
        "a line below the body's column defined something"
    );
    // The list spelling of the same control, corpus `350-3`, where the line is
    // published as the item's own text.
    assert_eq!(
        html("- a\n [r]: /u\ntail\n\n[r][]\n"),
        "<ul>\n  <li>a\n[r]: /u\ntail</li>\n</ul>\n<p>[r][]</p>"
    );
}

/// An ABBREVIATION definition is not on I5's list and is recognized at document
/// level only, so at the same column it is ordinary prose that REOPENS the
/// paragraph. Corpus `350-4` is the list spelling; this is the `dd`. A change
/// that suppressed by container rather than by construct would move it.
#[test]
fn an_abbreviation_definition_at_that_column_is_ordinary_prose() {
    assert_eq!(
        html(":: t\n:  a\n   *[A]: x\ntail\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>a\n*[A]: x\ntail</dd>\n</dl>"
    );
    assert_eq!(
        html("- a\n  *[A]: x\ntail\n"),
        "<ul>\n  <li>a\n*[A]: x\ntail</li>\n</ul>"
    );
}

/// THE COMMENT'S LIST-ITEM SPELLING IS NOT DECIDED, and this engine must not
/// decide it as a side effect. `- a` / `  %% c` / `tail` folds in all three
/// engines and in the executable spec while the `dd` spelling one construct over
/// does not, and markup-carve/carve#1358 records that division rather than
/// closing it: §17 L1a refuses to let an invisible construct have a visible
/// effect (markup-carve/carve#625) and the argument cuts both ways. Both answers
/// are pinned by the corpus today, so this row is an INTENDED SURVIVOR - it goes
/// red only when someone unifies the two containers without a ruling.
#[test]
fn the_list_item_spelling_of_the_comment_still_folds() {
    let out = html("- a\n  %% c\ntail\n");
    assert!(
        out.contains("tail") && !out.contains("<p>tail</p>"),
        "the list item's answer to a comment was changed without a ruling:\n{out}"
    );
}

/// A bare `:  ` line with no term above it opens no definition body, so it
/// claims no content column and a line three in defines nothing. Without the
/// open-list condition the prepass would hand a column to any line that happens
/// to start with a colon and two spaces.
#[test]
fn a_colon_line_that_opens_no_definition_body_claims_no_column() {
    assert!(
        !html("p\n:  a\n   [r]: /u\n\n[r][]\n").contains("href=\"/u\""),
        "a column was claimed where no definition body was opened"
    );
}

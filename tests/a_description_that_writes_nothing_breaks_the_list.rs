//! A declared loss is a CEILING, not a licence: an importer may lose what it
//! declares AND NO MORE, and it may add nothing at all
//! (markup-carve/carve#1608, carve#1627, carve#1636; `docs/html-import.md`).
//!
//! Carve has no spelling for an empty description. Six candidates were probed on
//! the ruling and none works - `: `, `: `, `: {}` and a tab after the colon each
//! leak a `:` into the text or fold into the term above, and a colon plus three
//! spaces yields `<dd>&nbsp;</dd>`, which is not empty. The bare colon line this
//! engine used to write is the worst of them: the parser reads it as more of the
//! TERM, so `<dl><dt>term</dt><dd></dd></dl>` came back as a `<dt>` reading
//! `term\n:` with no `<dd>` at all. The description was lost AND the term was
//! damaged, and the damage is a loss the row does not declare.
//!
//! The import writes the term alone, with `structure-unspellable` on the `<dd>`.
//!
//! THE CEILING BINDS IN THE OTHER DIRECTION TOO. Put an entry AFTER the dropped
//! one and writing both terms into one list gives the first the second's
//! description, because consecutive `::` lines SHARE the description written
//! below them. An ADDITION is not a loss and no row can declare it: a reader told
//! the empty description was dropped has been told nothing about `t1` acquiring
//! `d2`. So the list BREAKS at the dropped entry, and the grouping that is lost
//! takes its own row, `structure-split`.
//!
//! Both halves live in the WRITER, because the empty description survives into
//! the AST intact and only a writer loses it - which is also what makes an HTML
//! import, an ingested AST and `fmt` over parsed source take the same branch.
//! carve-js does the same in `renderDefinitionList`. The IMPORTER owns only the
//! two diagnostics, since it is the side that can name the `<dl>` and the `<dd>`.

fn imported(html: &str) -> carve::HtmlImportResult<String> {
    carve::html_to_carve(html, &carve::HtmlImportOptions::default()).expect("import")
}

fn rows(result: &carve::HtmlImportResult<String>) -> Vec<String> {
    result
        .report
        .diagnostics
        .iter()
        .map(|d| format!("{}@{}", d.code.as_str(), d.path.clone().unwrap_or_default()))
        .collect()
}

// ---------------------------------------------------------------------------
// The premises. Without these the whole rule is idle, and each of them is a
// statement about the PARSER that the writer's choice rests on.
// ---------------------------------------------------------------------------

#[test]
fn the_premise_a_bare_colon_line_is_read_as_more_of_the_term() {
    // Why the old output was not merely lossy but damaging.
    assert_eq!(
        carve::to_html(":: term\n:\n"),
        "<dl>\n  <dt>term\n:</dt>\n</dl>"
    );
}

#[test]
fn the_premise_consecutive_terms_share_the_description_below_them() {
    // Why dropping an entry cannot simply continue the same list.
    assert_eq!(
        carve::to_html(":: t1\n:: t2\n: d2\n"),
        "<dl>\n  <dt>t1</dt>\n  <dt>t2</dt>\n  <dd>d2</dd>\n</dl>"
    );
}

#[test]
fn the_premise_a_blank_line_neither_ends_a_definition_list_nor_survives_a_format_pass() {
    // Why the separator cannot be a blank line, which is the obvious first
    // guess. It is ONE list with both terms sharing `d2` - the outcome the rule
    // forbids - and the canonical writer removes the blank line again, so even
    // that reading would not survive `fmt`.
    assert_eq!(
        carve::to_html(":: t1\n\n:: t2\n: d2\n"),
        "<dl>\n  <dt>t1</dt>\n  <dt>t2</dt>\n  <dd>d2</dd>\n</dl>"
    );
    assert_eq!(
        carve::to_carve(":: t1\n\n:: t2\n: d2\n"),
        ":: t1\n:: t2\n: d2\n"
    );
}

#[test]
fn the_premise_a_comment_line_ends_the_list_and_is_a_writer_fixed_point() {
    // Why it CAN be a comment: it renders nothing where it stands and it stays
    // where it was written, which of the constructs that render nothing only a
    // comment does both of.
    let source = ":: t1\n\n%%\n\n:: t2\n: d2\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt>t1</dt>\n</dl>\n<dl>\n  <dt>t2</dt>\n  <dd>d2</dd>\n</dl>"
    );
    assert_eq!(carve::to_carve(source), source);
}

// ---------------------------------------------------------------------------
// The one-entry shape (carve#1627).
// ---------------------------------------------------------------------------

#[test]
fn a_dropped_last_entry_writes_the_term_alone_and_declares_the_description() {
    let result = imported("<dl><dt>term</dt><dd></dd></dl>");
    assert_eq!(result.value, ":: term\n");
    assert_eq!(rows(&result), ["structure-unspellable@/dl[1]/dd[2]"]);
    // The ceiling: exactly the empty description is gone, and the TERM is
    // undamaged - which is what the bare `:` line cost.
    assert_eq!(
        carve::to_html(&result.value),
        "<dl>\n  <dt>term</dt>\n</dl>"
    );
}

// ---------------------------------------------------------------------------
// The second side of the ceiling (carve#1636).
// ---------------------------------------------------------------------------

#[test]
fn a_dropped_entry_with_one_after_it_breaks_the_list() {
    let result = imported("<dl><dt>t1</dt><dd></dd><dt>t2</dt><dd>d2</dd></dl>");
    assert_eq!(result.value, ":: t1\n\n%%\n\n:: t2\n: d2\n");
    // Document order: the `<dl>` before the `<dd>` that is gone.
    assert_eq!(
        rows(&result),
        [
            "structure-split@/dl[1]",
            "structure-unspellable@/dl[1]/dd[2]"
        ]
    );
}

#[test]
fn the_surviving_term_does_not_acquire_the_next_entrys_description() {
    // The assertion the whole rule exists for, stated over the RE-RENDER rather
    // than over the source: `t1` keeps having no description, `t2` keeps exactly
    // `d2`, and nothing gains meaning it did not have.
    let written = imported("<dl><dt>t1</dt><dd></dd><dt>t2</dt><dd>d2</dd></dl>").value;
    assert_eq!(
        carve::to_html(&written),
        "<dl>\n  <dt>t1</dt>\n</dl>\n<dl>\n  <dt>t2</dt>\n  <dd>d2</dd>\n</dl>"
    );
}

#[test]
fn the_break_survives_a_format_pass() {
    // A separator that re-merged on `fmt` would put `d2` back under `t1` one
    // pass later, so being a fixed point is part of the fix rather than a nicety.
    let written = imported("<dl><dt>t1</dt><dd></dd><dt>t2</dt><dd>d2</dd></dl>").value;
    assert_eq!(carve::to_carve(&written), written);
}

#[test]
fn every_dropped_entry_with_a_term_after_it_breaks() {
    let result =
        imported("<dl><dt>t1</dt><dd></dd><dt>t2</dt><dd></dd><dt>t3</dt><dd>d3</dd></dl>");
    assert_eq!(result.value, ":: t1\n\n%%\n\n:: t2\n\n%%\n\n:: t3\n: d3\n");
    assert_eq!(
        rows(&result),
        [
            "structure-split@/dl[1]",
            "structure-unspellable@/dl[1]/dd[2]",
            "structure-unspellable@/dl[1]/dd[4]"
        ]
    );
}

#[test]
fn the_mark_is_spent_only_on_a_term_so_a_second_description_does_not_break() {
    // `<dl><dt>t</dt><dd></dd><dd>d2</dd></dl>` is ONE entry whose term already
    // has `d2`. Breaking here would strand `: d2` outside the list, where it
    // re-reads as a paragraph - a loss the rule was meant to prevent, not cause.
    let result = imported("<dl><dt>t</dt><dd></dd><dd>d2</dd></dl>");
    assert_eq!(result.value, ":: t\n: d2\n");
    assert_eq!(rows(&result), ["structure-unspellable@/dl[1]/dd[2]"]);
    assert!(
        !result.value.contains("%%"),
        "expected no break, got {:?}",
        result.value
    );
}

// ---------------------------------------------------------------------------
// "WRITES NOTHING", not "is empty".
// ---------------------------------------------------------------------------

#[test]
fn a_description_holding_only_layout_writes_nothing_and_takes_the_same_branch() {
    // The shape that broke carve-php: there, a DOM-shaped predicate let
    // `<dd><p> </p></dd>` split the list while declaring nothing. THIS engine
    // reaches the right answer by a different route - PART 11 §7 drops a
    // layout-only paragraph on the way in, so the description arrives with no
    // children at all and even a tree-shaped predicate would agree. The case is
    // pinned anyway, because §7's drop is what makes the two agree and nothing
    // else says so; the test below is the one that actually discriminates.
    let result = imported("<dl><dt>t1</dt><dd><p> </p></dd><dt>t2</dt><dd>d2</dd></dl>");
    assert_eq!(result.value, ":: t1\n\n%%\n\n:: t2\n: d2\n");
    assert!(
        rows(&result).contains(&"structure-split@/dl[1]".to_string()),
        "{:?}",
        rows(&result)
    );
    assert!(
        rows(&result).contains(&"structure-unspellable@/dl[1]/dd[2]".to_string()),
        "{:?}",
        rows(&result)
    );
}

/// THE TEST THAT PINS THE PREDICATE ITSELF.
///
/// An empty `<ul>` imports as a `list` node with no items - a real block, so
/// the description is NOT empty by shape - and it writes no line. Swapping
/// `writes_nothing` for `children.is_empty()` reddens exactly this test and
/// nothing else in the file, which is what makes it the discriminating case
/// rather than the layout-paragraph one above.
#[test]
fn a_description_holding_an_empty_list_writes_nothing_too() {
    let result = imported("<dl><dt>t1</dt><dd><ul></ul></dd><dt>t2</dt><dd>d2</dd></dl>");
    assert_eq!(result.value, ":: t1\n\n%%\n\n:: t2\n: d2\n");
    assert_eq!(
        rows(&result),
        [
            "structure-split@/dl[1]",
            "structure-unspellable@/dl[1]/dd[2]"
        ]
    );
}

// ---------------------------------------------------------------------------
// The side that must not move.
// ---------------------------------------------------------------------------

#[test]
fn an_ordinary_definition_list_is_untouched_and_declares_nothing() {
    let result = imported("<dl><dt>t1</dt><dd>d1</dd><dt>t2</dt><dd>d2</dd></dl>");
    assert_eq!(result.value, ":: t1\n: d1\n:: t2\n: d2\n");
    assert!(rows(&result).is_empty(), "{:?}", rows(&result));
    assert!(!result.value.contains("%%"));
}

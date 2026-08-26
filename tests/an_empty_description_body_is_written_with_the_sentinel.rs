//! A definition description whose body holds no blocks is written `: {empty}`,
//! the sentinel PART 11 §7b already uses for an empty footnote definition
//! (markup-carve/carve#1827).
//!
//! The line is a block-attribute line: the block it would attach to does not
//! exist, so the parse consumes it and the description reads back holding
//! nothing. That makes it a fixed point in EVERY position - above a blank line,
//! above a flush-left paragraph, and at end of input - so the writer needs no
//! lookahead over what follows. `: +` renders an empty `<dd>` too, but a `+`
//! ATTACHES the column-0 block under it and is only empty with a blank line
//! after it.
//!
//! Because every entry writes its own description line, consecutive `::` lines
//! never end up sharing one: a `<dl>` writes back as ONE list with the grouping
//! it parsed from, and the HTML importer owes no row for an empty `<dd>`.

const NOT_LAST: &str = "<dl><dt>t1</dt><dd></dd><dt>t2</dt><dd>d2</dd></dl>";
const LAST: &str = "<dl><dt>t1</dt><dd>d1</dd><dt>t2</dt><dd></dd></dl>";

fn imported(html: &str) -> carve::HtmlImportResult<String> {
    carve::html_to_carve(html, &carve::HtmlImportOptions::default()).expect("import")
}

fn codes(result: &carve::HtmlImportResult<String>) -> Vec<String> {
    result
        .report
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

#[test]
fn writes_the_sentinel_for_a_description_holding_no_blocks() {
    assert_eq!(carve::to_carve(":: t\n: {empty}\n"), ":: t\n: {empty}\n");
}

#[test]
fn the_sentinel_renders_an_empty_description() {
    assert_eq!(
        carve::to_html(":: t\n: {empty}\n"),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>"
    );
}

/// A FIXED POINT WHERE THE WRITER'S OWN BLOCK SPACING ALREADY HOLDS. The
/// flush-left spelling gains the blank line every block pair gets, which is why
/// it is checked through the rendering below rather than byte for byte.
#[test]
fn the_sentinel_is_a_fixed_point() {
    for source in [":: t\n: {empty}\n", ":: t\n: {empty}\n\nflush\n"] {
        assert_eq!(carve::to_carve(source), source, "source: {source:?}");
    }
}

#[test]
fn the_rendering_does_not_move_across_a_round_trip() {
    for source in [
        ":: t\n: {empty}\n",
        ":: t\n: {empty}\n\nflush\n",
        ":: t\n: {empty}\nflush\n",
    ] {
        assert_eq!(
            carve::to_html(&carve::to_carve(source)),
            carve::to_html(source),
            "source: {source:?}"
        );
    }
}

/// NO LOOKAHEAD. A flush-left paragraph directly under the sentinel does not
/// attach to it, which is what disqualified `: +`.
#[test]
fn a_flush_left_paragraph_under_the_sentinel_stays_outside_the_description() {
    assert_eq!(
        carve::to_html(":: t\n: {empty}\nflush\n"),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>flush</p>"
    );
}

/// ONE LIST, FOUR CHILDREN, wherever the empty entry sits.
#[test]
fn a_list_whose_empty_entry_is_not_the_last_one_stays_whole() {
    let result = imported(NOT_LAST);
    assert_eq!(result.value, ":: t1\n: {empty}\n:: t2\n: d2\n");
    assert_eq!(
        carve::to_html(&result.value),
        "<dl>\n  <dt>t1</dt>\n  <dd></dd>\n  <dt>t2</dt>\n  <dd>d2</dd>\n</dl>"
    );
}

#[test]
fn a_list_whose_empty_entry_is_the_last_one_stays_whole() {
    let result = imported(LAST);
    assert_eq!(result.value, ":: t1\n: d1\n:: t2\n: {empty}\n");
    assert_eq!(
        carve::to_html(&result.value),
        "<dl>\n  <dt>t1</dt>\n  <dd>d1</dd>\n  <dt>t2</dt>\n  <dd></dd>\n</dl>"
    );
}

#[test]
fn an_empty_description_declares_no_loss() {
    for html in [NOT_LAST, LAST] {
        let rows = codes(&imported(html));
        assert!(
            !rows.iter().any(|c| c == "structure-unspellable"),
            "html: {html:?}, rows: {rows:?}"
        );
        assert!(
            !rows.iter().any(|c| c == "structure-split"),
            "html: {html:?}, rows: {rows:?}"
        );
    }
}

/// THE CONDITION IS "THIS ENTRY WRITES NOTHING", not "the description is
/// empty": a `<dd>` holding a paragraph of layout whitespace and one holding a
/// list with no items write nothing too, and take the sentinel alike.
#[test]
fn every_description_that_writes_nothing_takes_the_sentinel() {
    for html in [
        "<dl><dt>t</dt><dd></dd></dl>",
        "<dl><dt>t</dt><dd><p> </p></dd></dl>",
        "<dl><dt>t</dt><dd><ul></ul></dd></dl>",
    ] {
        assert_eq!(imported(html).value, ":: t\n: {empty}\n", "html: {html:?}");
    }
}

/// THE SENTINEL DOES NOT EAT CONTENT. It is a sentinel only where it is the
/// whole line and reads as a block-attribute line.
#[test]
fn an_escaped_or_accompanied_brace_run_stays_content() {
    for (source, html) in [
        (
            ":: t\n: \\{empty}\n",
            "<dl>\n  <dt>t</dt>\n  <dd>{empty}</dd>\n</dl>",
        ),
        (
            ":: t\n: {empty} x\n",
            "<dl>\n  <dt>t</dt>\n  <dd>{empty} x</dd>\n</dl>",
        ),
    ] {
        assert_eq!(carve::to_html(source), html, "source: {source:?}");
        assert_eq!(carve::to_carve(source), source, "source: {source:?}");
    }
}

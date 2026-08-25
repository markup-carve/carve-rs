//! AN HTML COMMENT IMPORTS AS A CARVE COMMENT (markup-carve/carve#1709).
//!
//! It was dropped in every mode with nothing reported. The usual reason this
//! importer drops something is that Carve has no spelling for the shape - and
//! that reason never applied here, because CARVE HAS COMMENTS. So the drop was
//! a choice to lose bytes the format can hold, in the mode whose whole job is
//! fidelity, and it was a choice nobody had made: no clause anywhere named it.
//!
//! THE POSITION DECIDES THE SPELLING AND THE COMMENT IS NOT RELOCATED. Among
//! blocks it is a block comment, whose fence widens the way a code fence does,
//! so no payload can close it early. Inside an inline run it is the delimited
//! form, and two payloads close THAT early: text holding the closer, and text
//! holding a blank line, which ends the paragraph the run is in. Those are
//! dropped with one row saying so, rather than truncated or escaped into the
//! form - a comment that came back shorter, or carrying characters the author
//! did not write, is a silent content change.

use carve::html_import::{html_to_carve, HtmlImportMode, HtmlImportOptions};

const MODES: [HtmlImportMode; 3] = [
    HtmlImportMode::Safe,
    HtmlImportMode::Semantic,
    HtmlImportMode::Roundtrip,
];

fn import(html: &str, mode: HtmlImportMode) -> (String, Vec<String>) {
    let options = HtmlImportOptions {
        mode,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).unwrap();
    let codes = result
        .report
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_owned())
        .collect();
    (result.value, codes)
}

fn every_mode(html: &str, expected: &str) {
    for mode in MODES {
        let (value, codes) = import(html, mode);
        assert_eq!(value, expected, "{mode:?}");
        // Nothing was lost, so nothing is said.
        assert!(codes.is_empty(), "{mode:?}: {codes:?}");
    }
}

#[test]
fn a_comment_between_two_blocks_is_a_block_comment() {
    every_mode("<p>a</p><!--note--><p>b</p>", "a\n\n%%%\nnote\n%%%\n\nb\n");
}

#[test]
fn a_comment_inside_a_run_is_the_delimited_inline_comment() {
    every_mode("<p>a<!--note-->b</p>", "a{% note %}b\n");
}

#[test]
fn the_run_a_comment_sits_in_is_not_split() {
    // THE TWO POSITIONS TOLD APART. A run that also carries text is a real
    // inline run: emitting the comment as a block here would put the words
    // either side of it into two paragraphs, which is the document saying
    // something it never said.
    every_mode("<div>text <!--n--> more</div>", "text {% n %} more\n");
}

#[test]
fn the_pretty_printers_whitespace_around_a_comment_is_layout() {
    // Otherwise the answer would depend on whether the author indented their
    // HTML: the same comment would be a block one in a minified document and an
    // inline one in a formatted one.
    every_mode("<p>a</p>\n<!--n-->\n<p>b</p>", "a\n\n%%%\nn\n%%%\n\nb\n");
}

#[test]
fn a_comment_that_is_the_whole_document_is_kept() {
    every_mode("<!--note-->", "%%%\nnote\n%%%\n");
}

#[test]
fn a_multi_line_comment_is_kept_whole() {
    every_mode(
        "<!--multi\nline\ncomment-->",
        "%%%\nmulti\nline\ncomment\n%%%\n",
    );
}

#[test]
fn the_block_fence_widens_past_a_payload_that_is_itself_a_fence_line() {
    // The reason the BLOCK form has no unspellable case: the writer widens, so
    // no payload can close the fence early.
    every_mode("<!--%%%%-->", "%%%%%\n%%%%\n%%%%%\n");
}

#[test]
fn an_inline_comment_holding_the_closer_is_dropped_and_says_so() {
    // Written into the delimited form it would end where the closer appears, so
    // the rest of the payload comes back as prose and the document says
    // something the author never wrote. Refused loudly instead.
    for mode in MODES {
        let (value, codes) = import("<p>a<!--has %} in-->b</p>", mode);
        assert_eq!(value, "ab\n", "{mode:?}");
        assert_eq!(codes, vec!["element-dropped".to_owned()], "{mode:?}");
    }
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let report = html_to_carve("<p>a<!--has %} in-->b</p>", &options)
        .unwrap()
        .report;
    let row = &report.diagnostics[0];
    assert_eq!(row.path.as_deref(), Some("/p[1]/comment()[2]"));
    assert!(row.message.contains("holds the comment closer"), "{row:?}");
}

#[test]
fn an_inline_comment_holding_a_blank_line_is_dropped_and_says_so() {
    // A blank line ends the paragraph the run is in, so both halves come back
    // as prose and the comment is gone.
    for mode in MODES {
        let (value, codes) = import("<p>a<!--x\n\ny-->b</p>", mode);
        assert_eq!(value, "ab\n", "{mode:?}");
        assert_eq!(codes, vec!["element-dropped".to_owned()], "{mode:?}");
    }
    let options = HtmlImportOptions::default();
    let report = html_to_carve("<p>a<!--x\n\ny-->b</p>", &options)
        .unwrap()
        .report;
    assert!(
        report.diagnostics[0].message.contains("holds a blank line"),
        "{:?}",
        report.diagnostics[0]
    );
}

#[test]
fn an_inline_comment_carrying_one_newline_is_kept() {
    // NOT one of the two unspellable payloads, and worth pinning apart from
    // them: a single newline inside the run is a soft wrap, so the comment
    // re-reads intact and refusing it would be a loss with no cause.
    every_mode("<p>a<!--x\ny-->b</p>", "a{% x\ny %}b\n");
}

#[test]
fn an_unspellable_inline_comment_is_not_relocated_to_the_block_form() {
    // Moving it would put text somewhere the author did not write it, and
    // `roundtrip` reading its own output would then find the document had
    // moved.
    //
    // THE DOCUMENT CARRIES A SPELLABLE BLOCK COMMENT TOO, and that is what makes
    // this an assertion rather than a formality: asserting the absence of a
    // block fence around the unspellable comment alone passes for an engine
    // that never wrote a block comment in its life. Here the block form IS
    // reached and written, so the only way the inline one could appear beside it
    // is a relocation.
    let (value, codes) = import(
        "<!--block--><p>a<!--has %} in-->b</p>",
        HtmlImportMode::Roundtrip,
    );
    assert_eq!(value, "%%%\nblock\n%%%\n\nab\n");
    assert!(!value.contains("has"), "{value}");
    assert_eq!(codes, vec!["element-dropped".to_owned()]);
}

#[test]
fn a_comment_inside_preserved_raw_bytes_is_left_alone() {
    // It reaches the output with the element, so there is nothing to import and
    // nothing to report about it.
    let (value, codes) = import(
        "<form onclick=\"x()\"><!--kept--></form>",
        HtmlImportMode::Roundtrip,
    );
    assert!(value.contains("<!--kept-->"), "{value}");
    assert_eq!(
        codes,
        vec!["attribute-preserved".to_owned(), "raw-preserved".to_owned()]
    );
}

#[test]
fn a_comment_between_two_list_items_is_kept_and_says_that_it_moved() {
    // A list holds items, so there is no Carve position BETWEEN two of them.
    // The comment is emitted ahead of the list, which is what every other stray
    // child of a list does here, and the move is declared rather than silent.
    let (value, codes) = import(
        "<ul><li>a</li><!--n--><li>b</li></ul>",
        HtmlImportMode::Safe,
    );
    assert_eq!(value, "%%%\nn\n%%%\n\n- a\n- b\n");
    assert_eq!(codes, vec!["element-unwrapped".to_owned()]);
    let report = html_to_carve(
        "<ul><li>a</li><!--n--><li>b</li></ul>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .report;
    let row = &report.diagnostics[0];
    assert_eq!(row.path.as_deref(), Some("/ul[1]/comment()[2]"));
    // `Info`, not `Warning`: a comment renders nothing in either language, so
    // the move costs a reader of the OUTPUT nothing.
    assert_eq!(row.severity.as_str(), "info");
}

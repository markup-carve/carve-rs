//! A list's non-`li` children left the document whole and in silence.
//!
//! The `ul` / `ol` arm of the HTML importer filtered its children down to
//! `<li>` and walked only those, so `<ul><div id="stray">z</div><li>a</li></ul>`
//! imported as one item, the text `z` was gone, and the report was EMPTY - not
//! `element-dropped`, not `element-unwrapped', nothing. The report is the only
//! place a reader could have learned it happened (carve-rs#1261).
//!
//! Two things are asserted here, and the second is why the first is not enough:
//! the content survives, and the report says the child left its place among the
//! items. A fix that only reported the loss would still lose the words; a fix
//! that only kept them would move a `<div>` out of a list with nothing saying
//! so.
//!
//! The assertions go through the parser as well as the emitted source, because
//! what the fix promises is a document that still holds the content - and
//! source that does not read back as that document would satisfy a string
//! assertion while failing the promise.

use carve::{
    html_to_carve, parse, render_html, HtmlImportDiagnosticCode, HtmlImportOptions,
    HtmlImportSeverity,
};

fn imported(html: &str) -> (String, Vec<(String, String, String)>) {
    let r = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let diags = r
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.message.clone(),
                d.path.clone().unwrap_or_default(),
            )
        })
        .collect();
    (r.value, diags)
}

fn reparsed(src: &str) -> String {
    render_html(&parse(src)).unwrap()
}

/// The measured case from the ticket. The div keeps its own element AND its id -
/// it is emitted through the ordinary block walk rather than unwrapped by hand,
/// so nothing about it is lost except its place among the items.
#[test]
fn a_stray_div_keeps_its_content_its_id_and_gets_a_diagnostic() {
    let (src, diags) = imported("<ul><div id=\"stray\">z</div><li>a</li></ul>");
    assert_eq!(src, "{#stray}\n:::\nz\n:::\n\n- a\n");
    assert_eq!(
        reparsed(&src),
        "<div id=\"stray\">\n  <p>z</p>\n</div>\n<ul>\n  <li>a</li>\n</ul>"
    );
    assert_eq!(
        diags,
        vec![(
            "element-unwrapped".to_string(),
            "A <div> inside <ul> kept its content but not its place among the items: it is emitted as blocks ahead of the list".to_string(),
            "/ul[1]/div[1]".to_string(),
        )]
    );
}

/// The path is the child's index among the LIST's children, not its index in
/// the filtered array - a stray after the only item is the second child and is
/// reported there (PART 12 §16).
#[test]
fn the_path_counts_among_the_lists_children() {
    let (src, diags) = imported("<ul><li>a</li><p>tail</p></ul>");
    assert_eq!(src, "tail\n\n- a\n");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].2, "/ul[1]/p[2]");
}

/// Bare text directly inside the list is the same loss without an element to
/// name, so it is reported too and comes back as the paragraph it needs.
#[test]
fn bare_text_directly_inside_a_list_is_reported_and_kept() {
    let (src, diags) = imported("<ul>z<li>a</li></ul>");
    assert_eq!(src, "z\n\n- a\n");
    assert_eq!(reparsed(&src), "<p>z</p>\n<ul>\n  <li>a</li>\n</ul>");
    assert_eq!(
        diags,
        vec![(
            "element-unwrapped".to_string(),
            "Text directly inside <ul> kept its content but not its place among the items: it is emitted as a paragraph ahead of the list".to_string(),
            "/ul[1]/text()[1]".to_string(),
        )]
    );
}

/// A MARGIN IS NOT A LOSS. Every pretty-printed list carries whitespace text
/// nodes between its items, and reporting those would put a warning on the
/// ordinary shape - which is the way a diagnostic stops being read at all.
#[test]
fn the_whitespace_of_a_pretty_printed_list_reports_nothing() {
    let (src, diags) = imported("<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>");
    assert_eq!(src, "- a\n- b\n");
    assert_eq!(diags, vec![]);
}

/// A COMMENT IS KEPT NOW, and it moves, so the move is said
/// (markup-carve/carve#1709). It used to be dropped outright, which is what
/// made "nothing to report" true here; the text of the comment is the
/// document's, Carve can hold it, and losing it was a choice nobody made.
///
/// `info` rather than the `warning` its text neighbour takes: a comment renders
/// nothing in either language, so the move costs a reader of the OUTPUT nothing
/// and a reader of the SOURCE one position.
#[test]
fn a_comment_between_items_is_kept_and_says_that_it_moved() {
    let (src, diags) = imported("<ul><li>a</li><!--note--><li>b</li></ul>");
    assert_eq!(src, "%%%\nnote\n%%%\n\n- a\n- b\n");
    assert_eq!(
        diags,
        vec![(
            "element-unwrapped".to_string(),
            "An HTML comment directly inside <ul> kept its text but not its place among the items: it is emitted as a comment ahead of the list".to_string(),
            "/ul[1]/comment()[2]".to_string(),
        )]
    );
}

/// An ACTIVE element is dropped, not kept, and says exactly that. Reporting it
/// as content emitted ahead of the list would tell the reader the script
/// survived somewhere, and it must not survive anywhere.
#[test]
fn a_script_inside_a_list_is_dropped_and_not_reported_as_kept() {
    let (src, diags) = imported("<ul><script>x()</script><li>a</li></ul>");
    assert_eq!(src, "- a\n");
    assert_eq!(
        diags,
        vec![(
            "element-dropped".to_string(),
            "Dropped active <script> element".to_string(),
            "/ul[1]/script[1]".to_string(),
        )]
    );
    assert!(!src.contains("x()"));
}

/// The list itself is unchanged by the rescue: an `<ol start>` still starts
/// where it said, and the stray sits ahead of it.
///
/// The stray carries an ID, and that is the point rather than decoration: the
/// div is here to be a stray CHILD, so it has to be one the div arm writes as a
/// container. An attribute-less one unwraps to its content
/// (markup-carve/carve#1578), which would leave this case asserting the div
/// arm's answer while claiming to be about the list's. The twin test in
/// carve-js has always spelled it this way; this port dropped the id, so it
/// pinned the div arm by accident.
#[test]
fn the_lists_own_semantics_survive_a_stray_child() {
    let (src, _) = imported("<ol start=\"3\"><div id=\"s\">z</div><li>a</li></ol>");
    assert_eq!(src, "{#s}\n:::\nz\n:::\n\n3. a\n");
    assert_eq!(
        reparsed(&src),
        "<div id=\"s\">\n  <p>z</p>\n</div>\n<ol start=\"3\">\n  <li>a</li>\n</ol>"
    );
}

/// A misplaced sublist - `<ul>` directly inside `<ul>`, which no valid HTML
/// spells and an editor export does - comes back as a list of its own rather
/// than as nothing.
#[test]
fn a_sublist_with_no_item_around_it_survives_as_a_list() {
    let (src, diags) = imported("<ul><ul><li>n</li></ul><li>a</li></ul>");
    assert_eq!(
        reparsed(&src),
        "<ul>\n  <li>n</li>\n</ul>\n<ul>\n  <li>a</li>\n</ul>"
    );
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].0, "element-unwrapped");
}

/// Severity: a moved child is a WARNING, not an info note. A consumer that
/// filters to warnings is the one that needs to know its document was
/// restructured.
#[test]
fn a_moved_child_is_a_warning() {
    let r = html_to_carve(
        "<ul><div>z</div><li>a</li></ul>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    let [d] = r.report.diagnostics.as_slice() else {
        panic!("expected one diagnostic, got {:?}", r.report.diagnostics);
    };
    assert_eq!(d.code, HtmlImportDiagnosticCode::ElementUnwrapped);
    assert_eq!(d.severity, HtmlImportSeverity::Warning);
}

//! `<dl>` had no branch in the HTML importer, so the element fell through to
//! the unwrapping path and every term and every definition became inline
//! content of one paragraph: a glossary imported as a run of words with no
//! separator between a term and its own definition. The slot was already
//! there: `DefinitionList` exists, the parser fills it from `::` and `:`
//! lines, and the canonical writer emits it (markup-carve/carve#1210 P3).
//!
//! The assertions go through the parser rather than stopping at the emitted
//! source: what the row promises is a definition list on the other side, and a
//! `::` line that does not read back as one would satisfy a source assertion
//! while failing the promise.

use carve::{
    html_to_ast, html_to_carve, parse, render_html, BlockNode, HtmlImportDiagnosticCode,
    HtmlImportOptions, HtmlImportSeverity,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

fn reparsed(html: &str) -> String {
    render_html(&parse(&imported(html))).unwrap()
}

#[test]
fn a_definition_list_survives_the_import() {
    let html = "<dl><dt>Carve</dt><dd>A markup language.</dd></dl>";
    assert_eq!(imported(html), ":: Carve\n: A markup language.\n");
    assert_eq!(
        reparsed(html),
        "<dl>\n  <dt>Carve</dt>\n  <dd>A markup language.</dd>\n</dl>"
    );
}

/// Several `<dt>` before a `<dd>` are one group with several terms, which is
/// the same grouping the parser builds from consecutive `::` lines - so an
/// imported list and a hand-written one produce the same tree.
#[test]
fn several_terms_share_one_definition() {
    let html = "<dl><dt>HTML</dt><dt>HyperText Markup Language</dt><dd>The web's document format.</dd></dl>";
    let carve::Document { children, .. } = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    let [BlockNode::DefinitionList(list)] = children.as_slice() else {
        panic!("expected one definition list, got {children:?}");
    };
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].terms.len(), 2);
    assert_eq!(list.items[0].definitions.len(), 1);
}

/// A new `<dt>` after a definition opens the next group rather than adding a
/// term to the one that just closed.
#[test]
fn a_term_after_a_definition_opens_the_next_group() {
    let html = "<dl><dt>One</dt><dd>First</dd><dt>Two</dt><dd>Second</dd></dl>";
    let carve::Document { children, .. } = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    let [BlockNode::DefinitionList(list)] = children.as_slice() else {
        panic!("expected one definition list, got {children:?}");
    };
    assert_eq!(list.items.len(), 2);
    assert!(list.items.iter().all(|i| i.terms.len() == 1));
}

/// HTML5 gives `dl` two content models and Word, Google Docs and several
/// editors emit the second, because a `div` per group is the one CSS grid can
/// style. Both spell the same list, so both import to the same source.
#[test]
fn a_div_grouped_list_imports_the_same_way() {
    let direct = "<dl><dt>Term</dt><dd>Definition</dd></dl>";
    let wrapped = "<dl><div><dt>Term</dt><dd>Definition</dd></div></dl>";
    assert_eq!(imported(wrapped), imported(direct));
    assert_eq!(
        imported(
            "<dl><dt>Plain</dt><dd>Direct</dd><div><dt>Wrapped</dt><dd>Grouped</dd></div></dl>"
        ),
        ":: Plain\n: Direct\n:: Wrapped\n: Grouped\n"
    );
}

/// The wrapper carries nothing the `::` form spells, but an id or a class on it
/// is still a loss - so it is stated rather than dropped in silence.
#[test]
fn the_group_wrappers_own_attributes_are_reported() {
    let result = html_to_ast(
        "<dl><div class=\"row\"><dt>Term</dt><dd>Definition</dd></div></dl>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![HtmlImportDiagnosticCode::AttributeDropped]
    );
    // CONTROL: a bare wrapper is not a loss and reports nothing.
    assert!(html_to_ast(
        "<dl><div><dt>Term</dt><dd>Definition</dd></div></dl>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .report
    .diagnostics
    .is_empty());
}

/// A definition holding blocks keeps them: the body goes through the block
/// walk, not the inline one, so two paragraphs stay two paragraphs.
#[test]
fn a_definition_keeps_its_block_content() {
    assert_eq!(
        reparsed("<dl><dt>Term</dt><dd><p>One</p><p>Two</p></dd></dl>"),
        "<dl>\n  <dt>Term</dt>\n  <dd>\n    <p>One</p>\n    <p>Two</p>\n  </dd>\n</dl>"
    );
}

/// One level of wrapper unwraps, which is the only level HTML5 allows. A `div`
/// inside the wrapper is not a group, and before this row a doubly-wrapped list
/// imported to nothing at all with no diagnostic - the silent shape the tracker
/// exists to remove.
#[test]
fn a_doubly_wrapped_group_is_reported_rather_than_lost() {
    let result = html_to_ast(
        "<dl><div><div><dt>A</dt><dd>B</dd></div></div></dl>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(result.value.children.is_empty());
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| (d.code, d.severity))
            .collect::<Vec<_>>(),
        vec![(
            HtmlImportDiagnosticCode::ElementDropped,
            HtmlImportSeverity::Warning
        )]
    );
}

/// Anything else between the terms is not definition-list content and has
/// nowhere to go, so it is dropped with a warning rather than in silence.
#[test]
fn a_stray_element_between_the_terms_is_reported() {
    let result = html_to_ast(
        "<dl><dt>A</dt><dd>B</dd><p>stray</p></dl>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![HtmlImportDiagnosticCode::ElementDropped]
    );
}

/// A `<dd>` before any `<dt>` is not valid HTML5, but a sliced-up editor export
/// produces one. It cannot become a group: a definition line under an empty
/// `::` reads back as a paragraph, so writing one would trade a silent loss for
/// a corrupt document. The content is emitted ahead of the list instead.
#[test]
fn a_definition_with_no_term_keeps_its_content_and_states_the_loss() {
    let html = "<dl><dd>Orphan</dd><dt>T</dt><dd>D</dd></dl>";
    assert_eq!(imported(html), "Orphan\n\n:: T\n: D\n");
    assert_eq!(
        reparsed(html),
        "<p>Orphan</p>\n<dl>\n  <dt>T</dt>\n  <dd>D</dd>\n</dl>"
    );
    let result = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .map(|d| (d.code, d.severity))
            .collect::<Vec<_>>(),
        vec![(
            HtmlImportDiagnosticCode::ElementUnwrapped,
            HtmlImportSeverity::Warning
        )]
    );
}

/// CONTROL. A representable list is a clean import: nothing about this row may
/// start reporting a loss on the shape it was written for.
#[test]
fn a_representable_list_reports_nothing() {
    let result = html_to_ast(
        "<dl><dt>Carve</dt><dd>A markup language.</dd></dl>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert!(
        result.report.diagnostics.is_empty(),
        "{:?}",
        result.report.diagnostics
    );
}

/// CONTROL. `<dl>` is now a block element, so a definition list between two
/// paragraphs must not swallow them or be swallowed into them.
#[test]
fn a_definition_list_between_paragraphs_stays_its_own_block() {
    assert_eq!(
        imported("<p>Before</p><dl><dt>T</dt><dd>D</dd></dl><p>After</p>"),
        "Before\n\n:: T\n: D\n\nAfter\n"
    );
}

/// The importer's node and depth limits are what make it safe on untrusted
/// HTML, and a walk of its own has to pay into them or they are limits the
/// input can step around. The group wrapper is the case that matters: it is
/// not a node in the result, but it is a level of nesting and of recursion.
#[test]
fn the_definition_list_walk_pays_into_the_importers_limits() {
    let many_terms = format!("<dl>{}</dl>", "<dt></dt>".repeat(64));
    assert_eq!(
        html_to_ast(
            &many_terms,
            &HtmlImportOptions {
                max_nodes: 8,
                ..Default::default()
            },
        )
        .unwrap_err(),
        carve::HtmlImportError::NodeLimit
    );

    // A group wrapper costs a level of depth: the same chain of definitions
    // fits without one and does not fit with one.
    //
    // The bound is 14 rather than the 18 it was because the walk no longer
    // descends through the `<html>`/`<head>`/`<body>` the HTML parser
    // synthesizes: those four levels used to be charged to every import,
    // whatever the input, so a caller's `max_depth` described the parser's
    // scaffolding as much as their own content (markup-carve/carve#1257). What
    // this test is about is unchanged - the wrapper still costs a level, and
    // the pair still straddles the limit.
    let nest = |open: &str, close: &str, n: usize| {
        let mut html = String::new();
        for _ in 0..n {
            html.push_str(open);
        }
        html.push('x');
        for _ in 0..n {
            html.push_str(close);
        }
        html
    };
    let options = HtmlImportOptions {
        max_depth: 14,
        ..Default::default()
    };
    assert!(html_to_ast(&nest("<dl><dd>", "</dd></dl>", 4), &options).is_ok());
    assert_eq!(
        html_to_ast(&nest("<dl><div><dd>", "</dd></div></dl>", 4), &options).unwrap_err(),
        carve::HtmlImportError::DepthLimit
    );
}

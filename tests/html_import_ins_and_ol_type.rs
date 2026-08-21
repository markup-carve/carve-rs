//! Two HTML-import mappings that were listed as handled and were not
//! (markup-carve/carve#1210 P7).
//!
//! `<ins>` had no branch, so an insertion fell through to the unwrapping path:
//! the element was lost AND reported as unsupported markup, though Carve spells
//! it `{+ +}` and renders that back to `<ins>`.
//!
//! `<ol type="a">` was on the list of attributes the importer does not report,
//! which read as "handled". Nothing ever set `List::ol_type`, so the style left
//! the document without a word and the list imported as decimal.

use carve::{
    html_to_ast, html_to_carve, parse, render_carve, render_html, BlockNode,
    HtmlImportDiagnosticCode, HtmlImportOptions, HtmlImportSeverity, OrderedListType,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

fn only_list(html: &str) -> carve::List {
    let doc = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    match doc.children.into_iter().next() {
        Some(BlockNode::List(list)) => list,
        other => panic!("expected a list, got {other:?}"),
    }
}

#[test]
fn an_insertion_keeps_its_element() {
    let html = "<p>a <ins>added</ins> b</p>";
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    assert_eq!(result.value, "a {+added+} b\n");
    assert!(
        result.report.diagnostics.is_empty(),
        "an element Carve can spell is not a loss: {:?}",
        result.report.diagnostics
    );
    assert_eq!(
        render_html(&parse(&result.value)).unwrap(),
        "<p>a <ins>added</ins> b</p>"
    );
}

/// The three-engine question this test parked has since been answered:
/// carve-js maps `del` to its `delete` node and carve-php spells it `{- -}`, so
/// carve-rs#1223 moved `<del>` onto `CriticDelete` here too. What stays true is
/// what this test was for - the `<ins>` branch did not move its twin quietly;
/// the twin moved on its own ruling. `a_deletion_survives_an_html_import` owns
/// the shape now.
#[test]
fn a_deletion_maps_to_the_node_that_renders_it_back() {
    assert_eq!(imported("<p>a <del>gone</del> b</p>"), "a {-gone-} b\n");
}

/// The items are bare text, so the lists import TIGHT (carve#1210,
/// corpus-convert 27) and the style still has to reach every marker. Before
/// that ruling shipped here the importer wrote every `<li>` as its own
/// paragraph, and these expectations carried the blank line that produced.
#[test]
fn an_ordered_lists_style_reaches_the_marker() {
    for (html, carve, style) in [
        (
            "<ol type=\"a\"><li>x</li><li>y</li></ol>",
            "a. x\nb. y\n",
            OrderedListType::LowerAlpha,
        ),
        (
            "<ol type=\"A\"><li>x</li><li>y</li></ol>",
            "A. x\nB. y\n",
            OrderedListType::UpperAlpha,
        ),
        (
            "<ol type=\"i\"><li>x</li><li>y</li></ol>",
            "i. x\nii. y\n",
            OrderedListType::LowerRoman,
        ),
        (
            "<ol type=\"I\" start=\"3\"><li>x</li><li>y</li></ol>",
            "III. x\nIV. y\n",
            OrderedListType::UpperRoman,
        ),
    ] {
        assert_eq!(imported(html), carve, "{html}");
        assert_eq!(only_list(html).ol_type, Some(style), "{html}");
    }
}

/// The style is in the MARKER, so it works at any depth. An attribute block
/// above the list would not: nothing writes one for a nested list, which is how
/// a nested `<ol type="i">` lost its style outright in the engine that did use
/// an attribute.
#[test]
fn a_nested_lists_style_survives_too() {
    let html = "<ol type=\"a\"><li>x<ol type=\"i\"><li>n</li><li>m</li></ol></li></ol>";
    assert!(
        render_html(&parse(&imported(html)))
            .unwrap()
            .contains("<ol type=\"i\">"),
        "{}",
        imported(html)
    );
}

/// `type="1"` is the decimal default and the plain marker already means it, so
/// it produces neither a style nor an attribute nor a diagnostic.
#[test]
fn the_decimal_default_is_left_alone() {
    for html in [
        "<ol type=\"1\"><li>x</li></ol>",
        "<ol><li>x</li></ol>",
        "<ol type=\"\"><li>x</li></ol>",
    ] {
        let result = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
        let Some(BlockNode::List(list)) = result.value.children.first() else {
            panic!("expected a list: {html}");
        };
        assert_eq!(list.ol_type, None, "{html}");
        assert!(list.attrs.is_none(), "{html}: {:?}", list.attrs);
        assert!(result.report.diagnostics.is_empty(), "{html}");
    }
}

/// Three shapes have markers this engine's own parser reads back as a different
/// list. The raw `type` is kept, which still renders the right `<ol>`, and the
/// diagnostic says the style did not reach the marker. Before this the value
/// was dropped in silence.
#[test]
fn an_unspellable_style_is_kept_raw_and_reported() {
    for (html, why) in [
        ("<ol type=\"a\" start=\"9\"><li>only</li></ol>", "lone i."),
        ("<ol type=\"i\" start=\"5\"><li>only</li></ol>", "lone v."),
        (
            "<ol type=\"i\" start=\"1000\"><li>only</li></ol>",
            "lone m.",
        ),
        (
            "<ol type=\"a\" start=\"26\"><li>z</li><li>past</li></ol>",
            "runs past z",
        ),
    ] {
        let result = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
        let Some(BlockNode::List(list)) = result.value.children.first() else {
            panic!("expected a list: {why}");
        };
        assert_eq!(list.ol_type, None, "{why}");
        assert_eq!(
            list.attrs
                .as_ref()
                .and_then(|a| a.key_values.get("type"))
                .map(String::as_str),
            Some(if html.contains("\"a\"") { "a" } else { "i" }),
            "{why}"
        );
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .map(|d| (d.code, d.severity))
                .collect::<Vec<_>>(),
            vec![(
                HtmlImportDiagnosticCode::RawPreserved,
                HtmlImportSeverity::Info
            )],
            "{why}"
        );
        // The kept attribute is not a consolation prize: the HTML still comes
        // out with the numbering the source asked for.
        assert!(
            render_html(&parse(&imported(html)))
                .unwrap()
                .contains(if html.contains("\"a\"") {
                    "type=\"a\""
                } else {
                    "type=\"i\""
                }),
            "{why}"
        );
    }
}

/// The invariant, over every style/start/length combination that fits in the
/// ranges above: a style either reaches `ol_type` and READS BACK as the same
/// list, or it is kept as a raw attribute and reported. Neither branch may
/// produce a silently decimal list, which is what every one of these did
/// before.
///
/// The membership of the two branches is measured here rather than asserted
/// from a rule, because the overlap between one-letter alphabetic markers and
/// Roman numerals is resolved by the parser and stated nowhere: this is what
/// keeps the guard honest if that resolution ever moves.
#[test]
fn every_style_either_reaches_the_marker_or_is_reported() {
    let mut spelled = 0usize;
    let mut kept = 0usize;
    for ty in ["a", "A", "i", "I"] {
        for start in [
            1usize, 2, 4, 5, 9, 10, 24, 25, 26, 27, 50, 100, 500, 1000, 1001,
        ] {
            for items in 1..=3usize {
                let body = "<li>x</li>".repeat(items);
                let html = format!("<ol type=\"{ty}\" start=\"{start}\">{body}</ol>");
                let result = html_to_ast(&html, &HtmlImportOptions::default()).unwrap();
                let Some(BlockNode::List(list)) = result.value.children.first() else {
                    panic!("expected a list: {html}");
                };
                let has_raw_type = list
                    .attrs
                    .as_ref()
                    .is_some_and(|a| a.key_values.contains_key("type"));
                match list.ol_type {
                    Some(style) => {
                        spelled += 1;
                        assert!(!has_raw_type, "{html}: spelled and kept raw");
                        assert!(result.report.diagnostics.is_empty(), "{html}");
                        let src = render_carve(&result.value).unwrap();
                        let back = parse(&src);
                        let Some(BlockNode::List(read)) = back.children.first() else {
                            panic!("{html}: {src:?} did not read back as a list");
                        };
                        assert_eq!(read.ol_type, Some(style), "{html}: {src:?}");
                        assert_eq!(read.start.unwrap_or(1), start, "{html}: {src:?}");
                        assert_eq!(read.items.len(), items, "{html}: {src:?}");
                    }
                    None => {
                        kept += 1;
                        assert!(has_raw_type, "{html}: style vanished with no attribute");
                        assert!(
                            result
                                .report
                                .diagnostics
                                .iter()
                                .any(|d| d.code == HtmlImportDiagnosticCode::RawPreserved),
                            "{html}: style vanished with no diagnostic"
                        );
                    }
                }
            }
        }
    }
    // Both branches must be exercised, or the invariant above is half a test.
    assert_eq!((spelled, kept), (124, 56));
}

/// A Roman marker past `MMMCMXCIX` is a run of `m` whose length is the start
/// value over a thousand, and `start` is an author-supplied integer - so
/// without a cap a twenty-byte attribute buys an arbitrarily large marker, once
/// per item. The cap is a resource bound as much as a legibility one.
#[test]
fn a_roman_list_past_the_classic_range_is_kept_raw() {
    // The boundary itself still reaches the marker.
    let inside = "<ol type=\"i\" start=\"3998\"><li>x</li><li>y</li></ol>";
    assert_eq!(only_list(inside).ol_type, Some(OrderedListType::LowerRoman));

    for html in [
        "<ol type=\"i\" start=\"4000\"><li>x</li><li>y</li></ol>",
        "<ol type=\"I\" start=\"1000000000000\"><li>x</li><li>y</li></ol>",
    ] {
        let result = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
        let Some(BlockNode::List(list)) = result.value.children.first() else {
            panic!("expected a list: {html}");
        };
        assert_eq!(list.ol_type, None, "{html}");
        assert!(
            list.attrs
                .as_ref()
                .is_some_and(|a| a.key_values.contains_key("type")),
            "{html}"
        );
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .any(|d| d.code == HtmlImportDiagnosticCode::RawPreserved),
            "{html}"
        );
        // The written source stays proportional to the input, which is the
        // whole point of the cap.
        assert!(imported(html).len() < 200, "{html}: {}", imported(html));
    }
}

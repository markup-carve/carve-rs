use super::common::footnote_fanout::source;
use carve::{html_to_carve, HtmlImportAdapter, HtmlImportOptions};

fn word(input: &str) -> String {
    html_to_carve(
        input,
        &HtmlImportOptions {
            adapter: HtmlImportAdapter::Word,
            ..Default::default()
        },
    )
    .unwrap()
    .value
}

#[test]
fn every_reference_binds_and_wrapped_backlinks_are_removed() {
    let value = word(&source("wrapped-backlinks", 16));
    assert_eq!(
        value.split("\n\n").next().unwrap().matches("[^1]").count(),
        16
    );
    assert!(value.contains("[^1]: Note."));
    assert!(!value.contains("back"));
}

#[test]
fn nested_backlinks_keep_their_cleanup_order() {
    let back = "<a href=\"#r\" class=\"footnote-back\">back</a>";
    let input = format!("<p>Body<a id=\"r\" href=\"#fn1\" role=\"doc-noteref\">1</a>.</p><p id=\"fn1\">Note.<span id=\"outer\">{back}<span>{back}</span>{back}</span></p>");
    assert_eq!(word(&input), "Body[^1].\n\n[^1]: Note.\n");
}

#[test]
fn many_aliases_and_repeated_fragments_share_one_definition() {
    let mut refs = String::new();
    let mut targets = String::new();
    for i in 0..20 {
        refs.push_str(&format!("<a href=\"#alias{i}\" role=\"doc-noteref\">1</a>"));
        targets.push_str(&format!(
            "<a id=\"alias{i}\" href=\"#unused\" class=\"footnote-back\">1</a>"
        ));
    }
    refs.push_str(&"<a href=\"#alias0\" role=\"doc-noteref\">1</a>".repeat(2));
    let value = word(&format!(
        "<p>{refs}</p><section><p>{targets}Note.</p></section>"
    ));
    assert_eq!(
        value.split("\n\n").next().unwrap().matches("[^1]").count(),
        22
    );
    assert!(value.ends_with("\n\n[^1]: Note.\n"));
}

#[test]
fn empty_wrappers_keep_definition_order_and_note_placement() {
    let input = "<p><a href=\"#b\" role=\"doc-noteref\">2</a><a href=\"#a\" role=\"doc-noteref\">1</a></p><div id=\"endnotes\"><div><p id=\"a\">First.</p></div> \n<!--layout--><div><p id=\"b\">Second.</p></div></div><p>Tail.</p>";
    let value = word(input);
    assert!(value.starts_with("[^2][^1]\n"));
    assert!(value.contains("%%%\nlayout\n%%%"));
    assert!(value.find("footnotes").unwrap() < value.find("Tail.").unwrap());
    assert!(value.find("[^1]: First.").unwrap() < value.find("[^2]: Second.").unwrap());
}

#[test]
fn repeated_reference_identities_keep_all_references() {
    let source = crate::common::footnote_fanout::source("duplicate-identities", 20);
    let options = carve::HtmlImportOptions {
        adapter: carve::HtmlImportAdapter::Word,
        ..Default::default()
    };
    let value = carve::html_to_carve(&source, &options).unwrap().value;
    assert_eq!(
        value.split("\n\n").next().unwrap().matches("[^1]").count(),
        20
    );
    assert!(!value.contains("back"));
}

#[test]
fn notes_beneath_a_deep_shared_wrapper_are_normalized() {
    let source = crate::common::footnote_fanout::source("deep-shared-wrapper", 64);
    let options = carve::HtmlImportOptions {
        adapter: carve::HtmlImportAdapter::Word,
        ..Default::default()
    };
    let value = carve::html_to_carve(&source, &options).unwrap().value;
    assert!(value.contains("[^64]: Note."));
    assert!(value.contains("Tail."));
}

#[test]
fn overlapping_backlink_blocks_keep_the_innermost_definition() {
    let value = word(&source("nested-backlink-blocks", 64));
    assert!(value.contains("[^1]: Note."));
    assert!(!value.contains("back"));
    assert!(value.contains("(#fn0)"));
}
#[test]
fn a_deep_body_is_still_rejected_after_alias_resolution() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let options = HtmlImportOptions {
                adapter: HtmlImportAdapter::Word,
                ..Default::default()
            };
            assert!(matches!(
                carve::html_to_ast(&source("deep-alias-targets", 512), &options),
                Err(carve::HtmlImportError::DepthLimit)
            ));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn removing_many_empty_code_spans_preserves_intervening_text() {
    let input = format!("<p>{}</p>", "<code></code><span>x</span>".repeat(20));
    assert_eq!(word(&input), format!("{}\n", "x".repeat(20)));
}
#[test]
fn unwrapping_many_nested_spans_preserves_their_text() {
    let input = format!(
        "<p><strong>{}</strong></p>",
        "<strong>x</strong> y ".repeat(20)
    );
    let value = word(&input);
    assert!(value.contains("x y x y"));
}

#[test]
fn dropping_layout_after_hard_breaks_keeps_all_breaks_and_spans() {
    let input = format!("<p>{}</p>", "<br> <em>x</em>".repeat(20));
    let result = carve::html_to_ast(&input, &Default::default()).unwrap();
    let carve::ast::BlockNode::Paragraph(paragraph) = &result.value.children[0] else {
        panic!("expected paragraph")
    };
    assert_eq!(paragraph.children.len(), 40);
    assert_eq!(
        paragraph
            .children
            .iter()
            .filter(|node| matches!(node, carve::ast::InlineNode::HardBreak(_)))
            .count(),
        20
    );
}

#[test]
fn a_shared_inverse_with_many_classes_keeps_all_references() {
    for shape in ["long-inverse-class", "long-inverse-ref-class"] {
        let value = word(&source(shape, 64));
        assert_eq!(
            value.split("\n\n").next().unwrap().matches("[^1]").count(),
            64
        );
        assert!(value.contains("[^1]: Note."));
        assert!(!value.contains("back"));
    }
}

#[test]
fn dropping_blank_table_rows_preserves_survivors_and_diagnostic_order() {
    let result = html_to_carve(
        "<table><tr><td></td></tr><tr><td>A</td></tr><tr><td></td></tr><tr><td>B</td></tr></table>",
        &Default::default(),
    )
    .unwrap();
    assert!(result.value.contains('A'));
    assert!(result.value.contains('B'));
    let losses: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| {
            row.message
                .starts_with("Dropped a row whose every cell is empty")
        })
        .collect();
    assert_eq!(losses.len(), 2);
    assert!(losses[0].path.as_deref().unwrap().ends_with("/tr[1]"));
    assert!(losses[1].path.as_deref().unwrap().ends_with("/tr[3]"));
}

#[test]
fn earlier_notes_are_hidden_from_later_title_id_inference() {
    let result = html_to_carve("<p><a href=\"#fn1\" role=\"doc-noteref\">1</a><a href=\"#fn2\" role=\"doc-noteref\">2</a></p><section><div id=\"fn1\"><aside class=\"admonition note\" aria-labelledby=\"adm-1\"><p class=\"admonition-title\" id=\"adm-1\">T1</p><p>x</p></aside></div><div id=\"fn2\"><aside class=\"admonition note\" aria-labelledby=\"adm-2\"><p class=\"admonition-title\" id=\"adm-2\">T2</p><p>y</p></aside></div></section>", &HtmlImportOptions { adapter: HtmlImportAdapter::Word, ..Default::default() }).unwrap();
    assert!(result
        .report
        .diagnostics
        .iter()
        .any(
            |row| row.path.as_deref() == Some("footnote[2]/aside[1]/p[1]")
                && row.message.starts_with("Dropped id")
        ));
}

#[test]
fn note_removal_releases_reserved_title_suffixes() {
    let result = html_to_carve("<p id=\"adm-1\">reserved<a href=\"#fn1\" role=\"doc-noteref\">1</a><a href=\"#fn2\" role=\"doc-noteref\">2</a></p><section><div id=\"fn1\"><p id=\"adm-1-2\">reserved</p><aside class=\"admonition note\" aria-labelledby=\"adm-1-3\"><p class=\"admonition-title\" id=\"adm-1-3\">T1</p><p>x</p></aside></div><div id=\"fn2\"><aside class=\"admonition note\" aria-labelledby=\"adm-1-2\"><p class=\"admonition-title\" id=\"adm-1-2\">T2</p><p>y</p></aside></div></section>", &HtmlImportOptions { adapter: HtmlImportAdapter::Word, ..Default::default() }).unwrap();
    assert!(!result
        .report
        .diagnostics
        .iter()
        .any(
            |row| row.path.as_deref() == Some("footnote[2]/aside[1]/p[1]")
                && row.message.starts_with("Dropped id")
        ));
}

#[test]
fn body_title_inference_uses_the_pruned_namespace() {
    let result = html_to_carve("<p><a href=\"#fn1\" role=\"doc-noteref\">1</a></p><aside class=\"admonition note\" aria-labelledby=\"adm-1-2\"><p class=\"admonition-title\" id=\"adm-1-2\">Body title.</p><p>Body.</p></aside><section id=\"adm-1\"><div id=\"fn1\"><aside class=\"admonition note\" aria-labelledby=\"adm-2\"><p class=\"admonition-title\" id=\"adm-2\">Note title.</p><p>Note.</p></aside></div></section>", &HtmlImportOptions { adapter: HtmlImportAdapter::Word, ..Default::default() }).unwrap();
    assert!(result
        .report
        .diagnostics
        .iter()
        .any(|row| row.path.as_deref() == Some("/aside[2]/p[1]")
            && row.message.starts_with("Dropped id")));
}

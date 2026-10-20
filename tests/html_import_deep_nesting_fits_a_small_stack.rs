//! Deep HTML import refuses or preserves at the depth guard instead of overflowing a 2 MiB debug stack (markup-carve/carve-rs#2978).

use carve::{html_to_ast, html_to_carve, HtmlImportError, HtmlImportMode, HtmlImportOptions};
use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

fn on_small_stack(
    html: String,
    mode: HtmlImportMode,
    writing: bool,
) -> Result<(), HtmlImportError> {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let options = HtmlImportOptions {
                mode,
                ..HtmlImportOptions::default()
            };
            if writing {
                html_to_carve(&html, &options).map(|_| ())
            } else {
                html_to_ast(&html, &options).map(|_| ())
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn nested(open: &str, close: &str, count: usize) -> String {
    format!("{}x{}", open.repeat(count), close.repeat(count))
}

fn parsed_element_depth(html: &str) -> usize {
    let dom = parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut stack: Vec<(Handle, usize)> = vec![(dom.document.clone(), 0)];
    let mut deepest = 0;
    while let Some((node, depth)) = stack.pop() {
        let depth = depth + usize::from(matches!(node.data, NodeData::Element { .. }));
        deepest = deepest.max(depth);
        stack.extend(
            node.children
                .borrow()
                .iter()
                .cloned()
                .map(|child| (child, depth)),
        );
    }
    deepest
}

#[test]
fn deep_block_containers_fit_a_two_mib_stack() {
    let shapes = [
        ("div", "<div>", "</div>"),
        ("blockquote", "<blockquote>", "</blockquote>"),
        ("ul", "<ul><li>", "</li></ul>"),
        ("ol", "<ol><li>", "</li></ol>"),
        ("section", "<section>", "</section>"),
        ("figure", "<figure>", "</figure>"),
        ("details", "<details>", "</details>"),
        ("dl", "<dl><dd>", "</dd></dl>"),
    ];
    for (name, open, close) in shapes {
        let html = nested(open, close, 128);
        assert!(
            parsed_element_depth(&html) > 16,
            "{name}: {}",
            parsed_element_depth(&html)
        );
        for mode in [
            HtmlImportMode::Safe,
            HtmlImportMode::Semantic,
            HtmlImportMode::Roundtrip,
        ] {
            let result = on_small_stack(html.clone(), mode, false);
            let expected = if name == "figure" && mode == HtmlImportMode::Roundtrip {
                Ok(())
            } else {
                Err(HtmlImportError::DepthLimit)
            };
            assert_eq!(result, expected, "{name}");
        }
    }
}

#[test]
fn deep_inline_spans_fit_a_two_mib_stack() {
    let html = format!("<p>{}</p>", nested("<span>", "</span>", 128));
    assert!(
        parsed_element_depth(&html) >= 128,
        "{}",
        parsed_element_depth(&html)
    );
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        assert_eq!(
            on_small_stack(html.clone(), mode, false),
            Err(HtmlImportError::DepthLimit)
        );
    }
}

#[test]
fn a_captioned_figure_past_the_depth_limit_is_preserved() {
    let html = format!(
        "<figure id=\"d\">{}<figcaption>Cap</figcaption></figure>",
        nested("<div>", "</div>", 200)
    );
    let result = on_small_stack(html, HtmlImportMode::Roundtrip, true);
    assert_eq!(result, Ok(()));
}

#[test]
fn a_block_holding_table_cell_fits_a_two_mib_stack() {
    let html = format!(
        "<table><tr><td>{}</td></tr></table>",
        nested("<blockquote>", "</blockquote>", 128)
    );
    let result = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let options = HtmlImportOptions {
                list_table_for_block_cells: true,
                ..HtmlImportOptions::default()
            };
            html_to_ast(&html, &options).map(|_| ())
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(result, Err(HtmlImportError::DepthLimit));
}

#[test]
fn the_effective_depth_boundary_is_unchanged() {
    for (open, close, accepted, refused) in
        [("<div>", "</div>", 63, 64), ("<span>", "</span>", 127, 128)]
    {
        assert_eq!(
            on_small_stack(
                nested(open, close, accepted),
                HtmlImportMode::Semantic,
                false
            ),
            Ok(())
        );
        assert_eq!(
            on_small_stack(
                nested(open, close, refused),
                HtmlImportMode::Semantic,
                false
            ),
            Err(HtmlImportError::DepthLimit)
        );
    }
}

use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};
use std::sync::OnceLock;

fn node_shape(node: &Handle, parent: &str, first: bool, last: bool, code: bool) -> Value {
    let (name, mut attributes) = match &node.data {
        NodeData::Document => ("#document".to_owned(), Vec::new()),
        NodeData::Element { name, attrs, .. } => (
            name.local.to_string(),
            attrs
                .borrow()
                .iter()
                .filter(|attr| attr.name.local.as_ref() != "data-delim")
                .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                .collect::<Vec<_>>(),
        ),
        NodeData::Text { contents } => {
            static SPACES: OnceLock<regex::Regex> = OnceLock::new();
            let contents = contents.borrow();
            let mut value = if code {
                contents.to_string()
            } else {
                SPACES
                    .get_or_init(|| regex::Regex::new(r"[\t\r\n ]+").unwrap())
                    .replace_all(contents.as_ref(), " ")
                    .into_owned()
            };
            if !code && matches!(parent, "p" | "li" | "blockquote" | "td" | "th") {
                if first {
                    value = value.trim_start_matches(' ').to_owned();
                }
                if last {
                    value = value.trim_end_matches(' ').to_owned();
                }
            }
            return json!(value);
        }
        _ => panic!("unexpected HTML node in image control"),
    };
    attributes.sort();
    let children = node.children.borrow();
    let meaningful = children
        .iter()
        .filter(|child| {
            !matches!(&child.data, NodeData::Text { contents } if
                matches!(name.as_str(), "#document" | "html" | "head" | "body" | "blockquote" | "ul" | "ol" | "li") &&
                contents.borrow().trim_matches([' ', '\t', '\r', '\n']).is_empty())
        })
        .collect::<Vec<_>>();
    let values = meaningful
        .iter()
        .enumerate()
        .map(|(index, child)| {
            node_shape(
                child,
                &name,
                index == 0,
                index + 1 == meaningful.len(),
                code || matches!(name.as_str(), "code" | "pre"),
            )
        })
        .collect::<Vec<_>>();
    json!([name, attributes, values])
}

fn shape(html: &str) -> Value {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    node_shape(&dom.document, "", true, true, false)
}

#[test]
fn collapsed_image_keys_match_native_child_text() {
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/djot_collapsed_image_keys.json")).unwrap();
    let mut failures = Vec::new();
    let image_destinations = regex::Regex::new(r#"<img\b[^>]*\bsrc="([^"]*)""#).unwrap();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let expected = row["html"].as_str().unwrap();
        let report = carve::migrate_djot(source);
        let unresolved = report.report.diagnostics.iter().any(|item| {
            item.message == "An unresolved Djot image reference loses its image element."
        });
        let blank = report.report.diagnostics.iter().any(|item| {
            item.message
                == "An image alternative text with a blank line cannot be spelled in Carve."
        });
        if unresolved != row["loss"].as_bool().unwrap()
            || blank != row["blankAlt"].as_bool().unwrap()
        {
            failures.push(json!({"source":source,"unresolved":unresolved,"blank":blank,"expected_loss":row["loss"],"expected_blank":row["blankAlt"]}));
        }
        if row["loss"] == true {
            let actual = carve::to_html(&report.value);
            let actual_destinations: Vec<_> = image_destinations
                .captures_iter(&actual)
                .map(|caps| caps[1].to_owned())
                .collect();
            let expected_destinations: Vec<_> = image_destinations
                .captures_iter(expected)
                .map(|caps| caps[1].to_owned())
                .collect();
            assert_eq!(actual_destinations, expected_destinations, "{source:?}");
        }
        if row["loss"] == true || row["blankAlt"] == true {
            continue;
        }
        let actual = carve::to_html(&report.value);
        if shape(&actual) != shape(&super::image_paragraphs_as_images(expected)) {
            failures.push(json!({"source":source,"converted":report.value,"actual":actual,"expected":expected}));
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        serde_json::to_string_pretty(&failures).unwrap()
    );
}

#[test]
fn escaped_collapsed_brackets_remain_literal() {
    let report = carve::migrate_djot("\\[a][]\n\n[a]: img");
    assert_eq!(carve::to_html(&report.value).trim(), "<p>[a][]</p>");
}

#[test]
fn multiline_keys_inside_list_quotes() {
    for source in [
        "- > ![a\n  > b][]\n\n[a b]: img",
        "> - ![a\n>   b][]\n\n[a b]: img",
    ] {
        let report = carve::migrate_djot(source);
        assert!(
            report.report.diagnostics.iter().all(|item| item.message
                != "An unresolved Djot image reference loses its image element."),
            "{source:?}: {:?}",
            report.report.diagnostics
        );
        let html = carve::to_html(&report.value);
        assert!(html.contains("alt=\"a\nb\""), "{html}");
        assert!(html.contains("src=\"img\""), "{html}");
    }
    let report = carve::migrate_djot("- > ![a\n  > {.x}\n  > b](img)");
    assert!(report.report.diagnostics.iter().any(|item| item.message
        == "An image alternative text with a blank line cannot be spelled in Carve."));
}

#[test]
fn ordinary_explicit_reference_names_remain_authored() {
    for label in ["api_v2", "Mark's blog"] {
        for used in [false, true] {
            let source = if used {
                format!("[docs][{label}]\n\n[{label}]: https://example.com")
            } else {
                format!("[{label}]: https://example.com")
            };
            let report = carve::migrate_djot(&source);
            assert!(
                report.value.contains(&format!("[{label}]:")),
                "{}",
                report.value
            );
            assert!(
                !report.value.contains("carve-djot-reference-"),
                "{}",
                report.value
            );
        }
    }
}

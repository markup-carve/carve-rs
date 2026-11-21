use carve::ast::owned::Node;
use carve::reader::read_tree_json;
use carve::{Options, parse_resolved, render, render_tree};
use std::sync::Arc;

#[test]
fn c2_a_135_c2_a_134_renderer_resolves_supplied_results() {
    let source = "# Title\n\n[t][zz] a[^n]\n\n[^n]: note\n";
    let mut tree = parse_resolved(source, &Options::default()).expect("tree");
    tree.stages = Some(vec!["referenceResolution".into(), "tableAlignment".into()]);
    for child in &mut tree.children {
        match child {
            Node::Heading(h) => h.generated_id = Some("custom".into()),
            Node::Paragraph(p) => {
                for inline in &mut p.children {
                    match inline {
                        Node::FootnoteRef(r) => r.number = Some(5),
                        Node::Link(l) => l.href = Some("/stale".into()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let html = render_tree(&tree, &Options::default()).expect("render");
    assert!(html.contains("<section id=\"Title\">"), "{html}");
    assert!(html.contains("[t][zz]"), "{html}");
    assert!(html.contains("fnref1"), "{html}");
    assert!(!html.contains("/stale"), "{html}");
}

#[test]
fn c2_h_009_c2_h_013_direction_field_replaces_key() {
    let json = concat!(
        r#"{"type":"document","astVersion":"2.0","children":["#,
        r#"{"type":"paragraph","dir":"rtl","attrs":{"keyValues":[["dir","ltr"]]},"#,
        r#""children":[{"type":"text","value":"x"}]}]}"#
    );
    let tree = read_tree_json(json).expect("tree");
    assert_eq!(
        render_tree(tree.document(), &Options::default()).expect("render"),
        "<p dir=\"rtl\">x</p>"
    );
}

#[test]
fn c2_h_009_inline_direction_follows_language() {
    let tree = serde_json::json!({"type":"document","astVersion":"2.0","children":[
        {"type":"paragraph","children":[{"type":"span","lang":"de","dir":"rtl","children":[
            {"type":"text","value":"x"}
        ]}]}
    ]});
    let tree = carve::reader::read_tree_value(tree).expect("tree");
    assert_eq!(
        render_tree(tree.document(), &Options::default()).expect("render"),
        "<p><span lang=\"de\" dir=\"rtl\">x</span></p>"
    );
}

#[test]
fn c2_h_162_style_pattern_and_c2_h_161_background() {
    assert_eq!(
        render(
            "{style=\"background:image-set(x)\"}\nHello",
            &Options::default()
        ),
        "<p style=\"\">Hello</p>"
    );
    let opts = Options {
        url_policy: carve::options::UrlPolicy::Allow(vec!["mailto".into()]),
        ..Options::default()
    };
    assert!(render("|{background=\"https://x.org/a.png\"} a |", &opts).contains("background=\"\""));
}

#[test]
fn c2_h_116_reserved_attribute_is_dropped() {
    let source = "[x]{data-carve-raw=html}";
    let got = carve::render_with_diagnostics(source, &Options::default());
    assert!(!got.html.contains("data-carve-raw"));
    assert!(got.codes().contains(&"carrier-attribute-dropped".into()));
}

#[test]
fn c2_h_133_unexpanded_include_is_bare_list_content() {
    assert_eq!(
        render("- {{ a.crv }}\n- b", &Options::default()),
        "<ul>\n  <li>{{ a.crv }}</li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn c2_h_133_unexpanded_include_obeys_loose_list_and_quote_forms() {
    let loose = render("- a\n\n- {{ a.crv }}", &Options::default());
    assert!(loose.contains("<li><p>{{ a.crv }}</p></li>"), "{loose}");
    let quote = render("> {{ a.crv }}", &Options::default());
    assert_eq!(quote, "<blockquote><p>{{ a.crv }}</p></blockquote>");
}

#[test]
fn c2_i_021_raw_name_begins_with_ascii_letter() {
    assert_eq!(
        render("`x`{=_a}", &Options::default()),
        "<p><code>x</code>{=_a}</p>"
    );
}

#[test]
fn c2_n_074_c2_a_025_copied_include_caption_note_is_numbered() {
    let resolver =
        Arc::new(|_: &carve::include::Request<'_>| Ok((b"code\n".to_vec(), "a.txt".into())));
    let options = Options {
        includes: Some(carve::include::Includes::new(resolver)),
        ..Options::default()
    };
    let html = render("{{ a.txt @code }}\n^ Listing #: x^[note]", &options);
    assert!(html.contains("fnref1"), "{html}");
    assert!(html.contains("<p>note<a href=\"#fnref1\""), "{html}");
}

#[test]
fn c2_h_200_cell_blocks_use_row_relative_indentation() {
    let json = serde_json::json!({
        "type":"document","astVersion":"2.0","children":[{"type":"table",
        "columns":[{"type":"table_column"}],"children":[{"type":"table_body","children":[
            {"type":"table_row","children":[
                {"type":"table_cell","header":false,"children":[
                    {"type":"list","marker":{"kind":"bullet","char":"-"},"spacing":"tight","children":[
                        {"type":"list_item","children":[{"type":"paragraph","children":[{"type":"text","value":"a"}]}]}
                    ]},
                    {"type":"paragraph","children":[{"type":"text","value":"b"}]}
                ]},
                {"type":"table_cell","header":false,"children":[
                    {"type":"paragraph","children":[{"type":"text","value":"c"}]}
                ]}
            ]}
        ]}]}]
    });
    let tree = carve::reader::read_tree_value(json).expect("tree");
    let html = render_tree(tree.document(), &Options::default()).expect("render");
    assert!(html.contains("<tr><td>\n      <ul>"), "{html}");
    assert!(
        html.contains("\n      <p>b</p>\n    </td><td><p>c</p></td></tr>"),
        "{html}"
    );
}

fn table_with_one_cell(children: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({"type":"document","astVersion":"2.0","children":[{
        "type":"table","columns":[{"type":"table_column"}],"children":[{
            "type":"table_body","children":[{"type":"table_row","children":[
                {"type":"table_cell","header":false,"children":children}
            ]}]
        }]
    }]})
}

#[test]
fn c2_h_200_empty_cell_stays_on_the_row() {
    let empty = serde_json::json!({"type":"cell_paragraph","children":[]});
    let tree = carve::reader::read_tree_value(table_with_one_cell(&[empty])).expect("tree");
    let html = render_tree(tree.document(), &Options::default()).expect("render");
    assert!(html.contains("<tr><td></td></tr>"), "{html}");
}

#[test]
fn c2_h_062_c2_h_200_captioned_image_cell_keeps_figure() {
    let source = carve::tree_json("![a](x.png)\n^ cap", &Options::default());
    let parsed: serde_json::Value = serde_json::from_str(&source).expect("source tree");
    let child = parsed["children"][0].clone();
    let tree = carve::reader::read_tree_value(table_with_one_cell(&[child])).expect("cell tree");
    let html = render_tree(tree.document(), &Options::default()).expect("render");
    assert!(
        html.contains("<figure") && html.contains("<figcaption>cap</figcaption>"),
        "{html}"
    );
}

#[test]
fn c2_b_015_c2_b_055_marker_between_open_list_columns_joins_deeper_list() {
    let input = "- outer\n  - inner\n - between";
    let html = render(input, &Options::default());
    assert!(
        html.contains("<li>inner</li>\n      <li>between</li>"),
        "{html}"
    );
    let codes: Vec<_> = carve::lint::lint(input, &carve::lint::LintOptions::default())
        .into_iter()
        .map(|finding| finding.code)
        .collect();
    assert!(
        codes.contains(&"list-marker-below-content-column"),
        "{codes:?}"
    );
    assert!(!codes.contains(&"uneven-list-indent"), "{codes:?}");
}

#[test]
fn c2_b_166_escaped_pipe_start_is_paragraph_text() {
    let tree = parse_resolved("\\| a \\|{title=\"x\\|y\"}", &Options::default()).expect("tree");
    assert!(matches!(tree.children.first(), Some(Node::Paragraph(_))));
}

#[test]
fn c2_b_055_body_between_marker_and_content_columns_is_reported() {
    // carve#6230: a task mark is item text, so `  second` sits at the content column.
    for (source, reported) in [
        ("- a\n\n b", true),
        ("- [ ] first\n\n second", true),
        ("- [ ] first\n\n  second", false),
    ] {
        let codes: Vec<_> = carve::lint::lint(source, &carve::lint::LintOptions::default())
            .into_iter()
            .map(|finding| finding.code)
            .collect();
        assert_eq!(
            codes.contains(&"list-item-body-below-content-column"),
            reported,
            "{source}"
        );
    }
}

#[test]
fn c2_h_090_c2_h_091_note_backlink_after_image_and_comment() {
    let image = render("a[^n]\n\n[^n]: ![a](x.png)", &Options::default());
    assert!(
        image.contains("<img src=\"x.png\" alt=\"a\">\n      <p><a href=\"#fnref1\""),
        "{image}"
    );
    let comment = render("a[^n]\n\n[^n]: Body.\n%% comment", &Options::default());
    assert!(comment.contains("<p>Body.<a href=\"#fnref1\""), "{comment}");
}

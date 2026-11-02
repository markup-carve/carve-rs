#[test]
fn source_block_images_keep_the_paragraph_in_ast() {
    for source in ["![a](b)\n", "> ![a](b)\n", "- ![a](b)\n"] {
        let tree: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        let node = if source.starts_with('>') {
            &tree["children"][0]["children"][0]
        } else if source.starts_with('-') {
            &tree["children"][0]["items"][0]["children"][0]
        } else {
            &tree["children"][0]
        };
        assert_eq!(node["type"], "paragraph", "{source}");
        assert_eq!(node["blockImage"], true, "{source}");
        assert_eq!(node["children"][0]["type"], "image", "{source}");
    }
}

#[test]
fn source_table_columns_stop_at_the_table_width() {
    for key in ["aligns=left,right", "valigns=top,bottom", "widths=50,50"] {
        let source = format!("{{{key}}}\n| a |\n");
        let tree: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(&source))).unwrap();
        assert_eq!(
            tree["children"][0]["columns"].as_array().unwrap().len(),
            1,
            "{source}"
        );
    }
}

#[test]
fn invalid_row_partitions_leave_one_table_body() {
    for key in ["footer-rows=>", "header-rows=no", "header-rows=2"] {
        let source = format!("{{{key}}}\n|= A |\n");
        let html = carve::render_html(&carve::parse(&source)).unwrap();
        assert!(html.contains("<tbody>"), "{source}: {html}");
        assert!(!html.contains("<thead>"), "{source}: {html}");
        assert!(
            html.contains("<th scope=\"row\">A</th>"),
            "{source}: {html}"
        );
    }
}

#[test]
fn multiline_reference_labels_keep_their_source_in_ast() {
    // carve#4680: the label claims its run, as a one-line label does.
    for (source, kind) in [("[a][b\nc]\n", "link"), ("![a][b\nc]\n", "image")] {
        let tree: serde_json::Value =
            serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        let nodes = tree["children"][0]["children"].as_array().unwrap();
        assert_eq!(nodes.len(), 1, "{source}: {nodes:?}");
        assert_eq!(nodes[0]["type"], kind, "{source}");
        assert_eq!(nodes[0]["rawRef"], source.trim_end(), "{source}");
    }
}

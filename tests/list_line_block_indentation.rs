use carve::BlockNode;

fn first_line_block(nodes: &[BlockNode]) -> Option<&carve::LineBlock> {
    for node in nodes {
        let found = match node {
            BlockNode::LineBlock(block) => return Some(block),
            BlockNode::List(list) => list
                .items
                .iter()
                .find_map(|item| first_line_block(&item.children)),
            BlockNode::BlockQuote(quote) => first_line_block(&quote.children),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

#[test]
fn nested_verse_keeps_the_same_content_as_top_level_verse() {
    for base in 0..=3 {
        for body in [
            " ---",
            "  # heading",
            " > quote",
            " ```",
            " ::: note",
            " :::: \n ---",
            "::::\n ---\n::::\n  # verse",
            " :::\n ---",
            "\t---",
            " ordinary",
            " ---\n\n  ---",
        ] {
            let standalone = format!("::: |\n{body}\n:::\n");
            let expected = carve::parse(&standalone);
            let nested = format!(
                "- item\n\n{}",
                standalone
                    .lines()
                    .map(|line| format!("{}{line}\n", " ".repeat(2 + base)))
                    .collect::<String>()
            );
            let marker_first = format!(
                "- {}",
                standalone
                    .lines()
                    .enumerate()
                    .map(|(index, line)| {
                        if index == 0 {
                            format!("{line}\n")
                        } else {
                            format!("  {line}\n")
                        }
                    })
                    .collect::<String>()
            );
            let quoted = nested
                .lines()
                .map(|line| format!("> {line}\n"))
                .collect::<String>();
            let deeper = format!(
                "- outer\n\n{}",
                nested
                    .lines()
                    .map(|line| format!("  {line}\n"))
                    .collect::<String>()
            );
            for nested in [nested, marker_first, quoted, deeper] {
                let actual = carve::parse(&nested);
                assert_eq!(
                    first_line_block(&actual.children),
                    first_line_block(&expected.children),
                    "{nested:?}"
                );
                assert_eq!(
                    carve::to_html(&carve::to_carve(&nested)),
                    carve::to_html(&nested),
                    "{nested:?}"
                );
            }
        }
    }
}

#[test]
fn the_original_hyphen_case_keeps_one_no_break_space() {
    let source = "- item\n\n  ::: |\n   ---\n  :::\n";
    assert_eq!(carve::to_html(source), "<ul>\n  <li>item\n    <div class=\"line-block\">\n      <p>&nbsp;—</p>\n    </div>\n  </li>\n</ul>");
}

#[test]
fn rebasing_resumes_after_the_line_block_closer() {
    for gap in ["", "\n"] {
        let source = format!("- item\n\n  ::: |\n   ---\n  :::\n{gap}   # after\n");
        let html = carve::to_html(&source);
        assert!(html.contains("<p>&nbsp;—</p>"), "{html}");
        assert!(html.contains("<h1 id=\"after\">after</h1>"), "{html}");
    }
}

#[test]
fn preserved_marker_text_keeps_its_source_span() {
    fn visit(value: &serde_json::Value, source: &str, found: &mut bool) {
        if value["type"] == "text" && value["value"] == "# verse" {
            *found = true;
            let pos = &value["pos"];
            let start = pos["startOffset"].as_u64().unwrap() as usize;
            let end = pos["endOffset"].as_u64().unwrap() as usize;
            assert_eq!(&source[start..end], "# verse");
        }
        match value {
            serde_json::Value::Object(object) => {
                for child in object.values() {
                    visit(child, source, found);
                }
            }
            serde_json::Value::Array(array) => {
                for child in array {
                    visit(child, source, found);
                }
            }
            _ => {}
        }
    }
    for pad in ["  ", "   ", "    ", "     ", "\t", "\t "] {
        for body in [
            format!("{pad} # verse"),
            format!("{pad}\t# verse"),
            "\t\t# verse".into(),
        ] {
            let source = format!("- item\n\n{pad}::: |\n{body}\n{pad}:::\n");
            let doc =
                carve::parse_with_options(&source, &carve::Options::new().with_positions(true));
            let span = first_line_block(&doc.children)
                .unwrap()
                .pos
                .as_ref()
                .unwrap();
            assert_eq!(
                span.start_offset,
                source.find("::: |").unwrap(),
                "{source:?}"
            );
            assert_eq!(
                span.end_offset,
                source.rfind(":::").unwrap() + 3,
                "{source:?}"
            );
            let tree = serde_json::from_str(&carve::ast_json::to_json(&doc)).unwrap();
            let mut found = false;
            visit(&tree, &source, &mut found);
            assert!(found, "{source:?}");
        }
    }
}

#[test]
fn an_unclosed_line_block_keeps_marker_shaped_body_indentation() {
    for base in 0..=3 {
        let pad = " ".repeat(2 + base);
        let source = format!("- item\n\n{pad}::: |\n{pad} ---\n{pad}  # verse\n");
        let expected = carve::parse("::: |\n ---\n  # verse\n");
        let actual = carve::parse(&source);
        assert_eq!(
            first_line_block(&actual.children),
            first_line_block(&expected.children),
            "{source:?}"
        );
    }
}

#[test]
fn below_base_payload_keeps_its_residual_indent() {
    for text in ["x", ":::"] {
        let source = format!("- item\n\n     ::: |\n   {text}\n     :::\n");
        let html = carve::to_html(&source);
        assert!(html.contains(&format!("<p>&nbsp;{text}</p>")), "{html}");
    }
}

#[test]
fn shared_opener_classification_respects_other_container_columns() {
    for source in [
        ":: term\n: before\n\n   ::: |\n    # verse\n   :::\n",
        "[^n]: before\n\n   ::: |\n    # verse\n   :::\n\nref[^n]\n",
    ] {
        let html = carve::to_html(source);
        assert!(html.contains("<p>&nbsp;# verse</p>"), "{html}");
        assert!(!html.contains("<h1"), "{html}");
    }
    let lazy_quote = "> item\n ::: |\n  ---\n :::\n";
    let html = carve::to_html(lazy_quote);
    assert!(html.contains("::: |"), "{html}");
    assert!(!html.contains("class=\"line-block\""), "{html}");
}

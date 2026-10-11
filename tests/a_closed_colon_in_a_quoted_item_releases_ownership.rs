use serde_json::Value;

#[test]
fn a_closed_colon_releases_the_quote_before_an_unmarked_list() {
    for depth in 1..=3 {
        let quote = "> ".repeat(depth);
        for width in 3..=5 {
            let fence = ":".repeat(width);
            for body in ["", "body", "- nested"] {
                for indent in ["", "  "] {
                    let body = if body.is_empty() {
                        String::new()
                    } else {
                        format!("{quote}  {body}\n")
                    };
                    let source =
                        format!("{quote}- {fence} box\n{body}{quote}  {fence}\n{indent}- tail\n");
                    let ast: Value =
                        serde_json::from_str(&carve::to_json(&carve::parse(&source))).unwrap();
                    assert_eq!(
                        ast["children"].as_array().unwrap().len(),
                        2,
                        "{source}: {ast}"
                    );
                    assert_eq!(ast["children"][0]["type"], "block_quote", "{source}");
                    assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
                    assert!(
                        carve::to_html(&source).ends_with("<ul>\n  <li>tail</li>\n</ul>"),
                        "{source}"
                    );
                }
            }
        }
    }
}

#[test]
fn paragraph_and_explicit_quote_continuations_keep_their_owner() {
    for source in [
        "> - paragraph\n- tail\n",
        "> - paragraph\n  - tail\n",
        "> - ::: box\n>   :::\n> - tail\n",
        "> - ::: box\n>   :::\n+\n- tail\n",
        "> - ::: box\n>   :::\n> paragraph\n- tail\n",
    ] {
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            1,
            "{source}: {ast}"
        );
        assert_eq!(ast["children"][0]["type"], "block_quote");
        let html = carve::to_html(source);
        assert!(html.contains("tail"));
        if source.starts_with("> - paragraph") {
            assert!(html.contains("- tail"), "{source}: {html}");
        }
    }
}

#[test]
fn hosted_colon_interruption_and_dedent_do_not_keep_stale_claims() {
    for source in [
        "> - item\n>   ::: box\n>   :::\n- tail\n",
        "> - item\n>   ::: box\n>   body\n>   :::\n  - tail\n",
    ] {
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            2,
            "{source}: {ast}"
        );
        assert_eq!(ast["children"][1]["type"], "list");
    }
    for source in [
        ">   ::: box\n- tail\n",
        "> - ::: box\n>\n> para\n>   :::\n- tail\n",
        "> - :::box\n>   :::\n- tail\n",
        "> - paragraph\n>   :::box\n>   :::\n- tail\n",
    ] {
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            1,
            "{source}: {ast}"
        );
        assert!(carve::to_html(source).contains("- tail"), "{source}");
    }
}

#[test]
fn colon_shapes_inside_opaque_blocks_do_not_close_the_host() {
    for source in [
        "> - ```\n>   ::: box\n>   :::\n>   ```\n- tail\n",
        "> - %%%\n>   ::: box\n>   :::\n>   %%%\n- tail\n",
        "> - ::: box\n>   :::: |\n>   :::\n>   :::: \n>   paragraph\n- tail\n",
    ] {
        let html = carve::to_html(source);
        assert!(html.contains("tail"), "{source}: {html}");
        if source.contains("paragraph") {
            assert!(html.contains("- tail"), "{source}: {html}");
        }
    }
}

#[test]
fn a_verse_dedent_does_not_leave_a_stale_colon_owner() {
    let source = "> - ::: |\n>   literal\n>\n> paragraph\n>   :::\n- tail\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 1, "{ast}");
    assert!(carve::to_html(source).contains("- tail"));
}

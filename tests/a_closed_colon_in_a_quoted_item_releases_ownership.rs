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
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        if source.contains("paragraph") {
            assert_eq!(
                ast["children"].as_array().unwrap().len(),
                1,
                "{source}: {ast}"
            );
            assert!(html.contains("- tail"), "{source}: {html}");
        } else {
            assert_eq!(
                ast["children"].as_array().unwrap().len(),
                2,
                "{source}: {ast}"
            );
            assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
            let payload = &ast["children"][0]["children"][0]["items"][0]["children"][0];
            if source.contains("%%%") {
                assert_eq!(payload["type"], "comment", "{ast}");
                assert_eq!(payload["content"], "::: box\n:::", "{ast}");
            } else {
                assert_eq!(payload["type"], "code_block", "{ast}");
                assert_eq!(payload["content"], "::: box\n:::\n", "{ast}");
            }
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

#[test]
fn all_hosted_colon_block_kinds_and_typed_interruptions_release_ownership() {
    for marker in ["-", "1."] {
        let pad = " ".repeat(marker.len() + 1);
        for kind in ["box", ">", "\\", "|"] {
            for nested in [false, true] {
                let open = if nested {
                    format!("> {marker} ::: box\n> {pad}::: {kind}\n")
                } else {
                    format!("> {marker} ::: {kind}\n")
                };
                let close = if nested {
                    format!("> {pad}:::\n> {pad}:::\n")
                } else {
                    format!("> {pad}:::\n")
                };
                let source = format!("{open}> {pad}body\n{close}- tail\n");
                let ast: Value =
                    serde_json::from_str(&carve::to_json(&carve::parse(&source))).unwrap();
                assert_eq!(
                    ast["children"].as_array().unwrap().len(),
                    2,
                    "{source}: {ast}"
                );
                assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
            }
        }
    }
    let source = "> - :::box\n>   ::: note\n>   :::\n- tail\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 2, "{ast}");
}

#[test]
fn a_lazy_line_after_hosted_comment_shapes_keeps_existing_ownership() {
    let source = "> - %%%\n>   ::: box\n>   :::\n- tail\n>   %%%\n";
    let html = carve::to_html(source);
    assert!(html.ends_with("</blockquote>"), "{html}");
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 1, "{ast}");
    assert_eq!(
        ast["children"][0]["children"][0]["items"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "{ast}"
    );
}

#[test]
fn a_comment_inside_verse_keeps_colon_payload_opaque() {
    let source = "> - ::: |\n>   %%%\n>   :::\n>   %%%\n>   body\n>   :::\n- tail\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 2, "{ast}");
    assert_eq!(ast["children"][1]["type"], "list", "{ast}");
    assert!(carve::to_html(source).contains("body"));
}

#[test]
fn a_new_item_or_later_bare_block_does_not_inherit_absorption() {
    for source in [
        "> - prose\n>   :::bad\n> - next\n>   :::\n- tail\n",
        "> - ::: box\nlazy\n>   :::\n- tail\n",
        "> - ::: box\n>   body\nlazy\n>   :::\n- tail\n",
    ] {
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            2,
            "{source}: {ast}"
        );
        assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
    }
}

#[test]
fn unclosed_comments_do_not_hide_real_fences_or_colon_closers() {
    for source in [
        "> - %%%\n>   ```\n>   code\n>   ```\n- tail\n",
        "> - item\n>   %%%\n>   ```\n>   code\n>   ```\n- tail\n",
        "> - %%%\n>   ::: box\n>   :::\n- tail\n",
        "> - ::: |\n>   %%%\n>   body\n>   :::\n- tail\n",
        "> - ::: box\n>   body\n> lazy\n>   :::\n- tail\n",
        "> - a\n>   :::\n- tail\n",
        "> - a\n>   ::: box\n- tail\n",
    ] {
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            2,
            "{source}: {ast}"
        );
        assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
    }
}

#[test]
fn a_comment_closer_below_the_item_does_not_hide_its_code_fence() {
    let source = "> - %%%\n>   ```\n>   code\n>   ```\n- tail\n> %%%\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"][1]["type"], "list", "{ast}");
}

#[test]
fn dedented_interrupters_end_stale_hosts_and_colon_scopes() {
    for source in [
        "> - item\n> # heading\n>   ::: box\n>   :::\n- tail\n",
        "> - ::: box\n>   body\n> # heading\n>   :::\n- tail\n",
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
fn existing_fence_verdict_survives_hosted_opaque_shapes() {
    for depth in 1..=2 {
        let quote = "> ".repeat(depth);
        for body in [
            "- item\n{q}```\n{q}code\n{q}```\n- tail\n",
            "- %%%\n{q}     %%%\n{q}  ```\n{q}  code\n{q}  ```\n- tail\n",
            "- %%%\n{q}  ```\n{q}  code\n{q}  ```\n- tail\n{q}  %%%\n",
            "- ::: |\n{q}  ```\n{q}  code\n{q}  ```\n- tail\n",
        ] {
            let source = format!("{quote}{}", body.replace("{q}", &quote));
            let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(&source))).unwrap();
            assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
        }
    }
}

#[test]
fn a_future_comment_closer_after_a_host_dedent_does_not_mask_closed_colons() {
    let source = "> - %%%\n>   ::: box\n>   :::\n- tail\n> after\n>   %%%\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"][1]["type"], "list", "{ast}");
}

#[test]
fn a_sibling_item_ends_the_previous_colon_scope() {
    for source in [
        "> - a\n>   - ::: box\n>     body\n> - next\n>     :::\n- tail\n",
        "> - ::: box\n>   body\n> - :::bad\n>   :::\n- tail\n",
    ] {
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        let expected = if source.starts_with("> - a\n") { 2 } else { 1 };
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            expected,
            "{source}: {ast}"
        );
        if expected == 2 {
            assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
        } else {
            assert!(carve::to_html(source).contains("- tail"), "{source}");
        }
    }
}

#[test]
fn an_item_to_the_left_also_ends_the_previous_colon_scope() {
    for source in [
        ">  - ::: box\n>    body\n> - next\n>    :::\n- tail\n",
        "> - a\n>    - ::: box\n>      body\n>   - next\n>      :::\n- tail\n",
    ] {
        for nested in [false, true] {
            let source = if nested {
                source
                    .lines()
                    .map(|line| {
                        if line.starts_with('>') {
                            format!("> {line}\n")
                        } else {
                            format!("{line}\n")
                        }
                    })
                    .collect::<String>()
            } else {
                source.to_owned()
            };
            let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(&source))).unwrap();
            let expected = if source.contains("- a\n") { 2 } else { 1 };
            assert_eq!(
                ast["children"].as_array().unwrap().len(),
                expected,
                "{source}: {ast}"
            );
            if expected == 2 {
                assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
            } else {
                assert!(carve::to_html(&source).contains("- tail"), "{source}");
            }
        }
    }
}

#[test]
fn a_marker_below_an_ancestor_content_column_keeps_the_leaf_paragraph() {
    let base = "> - a\n>   - ::: box\n>     body\n>  - next\n>     :::\n- tail\n";
    for nested in [false, true] {
        let source = if nested {
            base.lines()
                .map(|line| {
                    if line.starts_with('>') {
                        format!("> {line}\n")
                    } else {
                        format!("{line}\n")
                    }
                })
                .collect::<String>()
        } else {
            base.to_owned()
        };
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(&source))).unwrap();
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            2,
            "{source}: {ast}"
        );
        assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
        assert!(carve::to_html(&source).contains("body\n- next"), "{source}");
    }
    let source = "> - a\n>   - ::: box\n>     body\n>    - ::: inner\n>       :::\n- tail\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 1, "{ast}");
    assert!(carve::to_html(source).contains("- tail"));
}

#[test]
fn a_marker_between_deeper_ancestor_columns_keeps_the_closed_leaf_owner() {
    let source =
        "> - a\n>   - b\n>     - ::: box\n>       body\n>    - next\n>       :::\n- tail\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 2, "{ast}");
    assert_eq!(ast["children"][1]["type"], "list", "{ast}");
    assert!(carve::to_html(source).contains("body\n- next"));
}

#[test]
fn an_ancestor_colon_closer_and_a_folded_malformed_marker_leave_no_claim() {
    for source in [
        "> - ::: box\n>   - ::: inner\n>     body\n>   :::\n- tail\n",
        "> - a\n>   - b\n>  - :::bad\n>   :::\n- tail\n",
    ] {
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            2,
            "{source}: {ast}"
        );
        assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
    }
}

#[test]
fn a_closed_hosted_comment_ends_the_paragraph_before_quoted_dedent() {
    let source = "> - ::: box\n>   text\n>   %%%\n>   c\n>   %%%\n> out\n>   :::\n- tail\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 1, "{ast}");
    assert!(carve::to_html(source).contains("out\n:::\n- tail"));
}

#[test]
fn a_closed_marker_line_colon_releases_an_unquoted_nested_item() {
    let source = "- a\n  - ::: box\n    body\n - next\n    :::\ntail\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 2, "{ast}");
    assert_eq!(ast["children"][1]["type"], "paragraph", "{ast}");
    assert!(carve::to_html(source).contains("body\n- next"));
}
#[test]
fn a_new_item_left_of_the_first_quoted_marker_registers_its_colon_host() {
    let source = ">  - a\n> - next\n>   ::: b\n>   x\n>   :::\ntail\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 2, "{ast}");
    assert_eq!(ast["children"][1]["type"], "paragraph", "{ast}");
}

#[test]
fn an_owned_verse_opener_clears_absorption_and_leaves_no_paragraph_claim() {
    for source in [
        "> - a\n>   ::: |\n- tail\n",
        "> - a\n>   - p\n>     :::bad\n>     ::: |\n>     v\n>   text\n>   :::\n- tail\n",
    ] {
        let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
        assert_eq!(
            ast["children"].as_array().unwrap().len(),
            2,
            "{source}: {ast}"
        );
        assert_eq!(ast["children"][1]["type"], "list", "{source}: {ast}");
    }
}

#[test]
fn a_future_comment_pair_cannot_hide_an_actual_closed_verse() {
    let source = "> - ::: |\n>   %%%\n>   :::\n- tail\n>   %%%\n>   :::\n";
    let ast: Value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(ast["children"].as_array().unwrap().len(), 3, "{ast}");
    assert_eq!(ast["children"][1]["type"], "list", "{ast}");
    assert_eq!(
        ast["children"][0]["children"][0]["items"][0]["children"][0]["type"], "line_block",
        "{ast}"
    );
}

#[test]
fn transparent_comment_preserves_the_hosted_closer() {
    let source = "> - ::: box\n>   a\n> %% note\n>   :::\n- tail\n";
    assert!(
        carve::to_html(source).ends_with("<ul>\n  <li>tail</li>\n</ul>"),
        "{}",
        carve::to_html(source)
    );
}

#[test]
fn off_column_bare_run_keeps_the_hosted_paragraph_open() {
    let source = "> - ::: box\n>\n>    :::\n- tail\n";
    assert!(
        carve::to_html(source).contains(":::\n- tail</p>"),
        "{}",
        carve::to_html(source)
    );
}

#[test]
fn quoted_hosted_colon_followups_match_shared_html() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/quoted_hosted_colon_followups.json")).unwrap();
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let actual = carve::to_html(source);
        assert_eq!(
            whitespace.replace_all(&actual, "><").trim(),
            case["html"].as_str().unwrap(),
            "{source}"
        );
    }
}

#[test]
fn closed_code_inside_descendant_verse_preserves_its_real_closer() {
    let cases: Value = serde_json::from_str(include_str!(
        "fixtures/descendant_literal_code_ownership.json"
    ))
    .unwrap();
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            whitespace.replace_all(&carve::to_html(source), "><").trim(),
            whitespace
                .replace_all(case["html"].as_str().unwrap(), "><")
                .trim(),
            "{source}",
        );
    }
}

#[test]
fn quoted_colon_remaining_controls_match_shared_html() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/quoted_colon_remaining_parity.json")).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 173);
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let expected = case["html"].as_str().unwrap();
        for actual in [
            carve::to_html(source),
            carve::render_html(&carve::parse(source)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&actual, "><").trim(),
                expected,
                "{}: {source}",
                case["id"]
            );
        }
    }
}

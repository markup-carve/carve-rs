use carve::{djot_to_carve, to_html};

#[test]
fn rejected_block_comments_keep_visible_text_and_heading_ownership() {
    for source in [
        "{% x\ny %}\n",
        "# a\n{% x\ny %}\n# b\n",
        "> {% x\n> y %}\n",
        "- {% x\n  y %}\n",
    ] {
        let html = to_html(&djot_to_carve(source));
        assert!(html.contains('x') && html.contains('y'), "{html}");
        assert!(!html.contains("<h1>b</h1>"), "{html}");
        if source.starts_with('#') {
            assert!(html.contains("<h1>a</h1>"), "{html}");
        }
    }
}

#[test]
fn valid_comment_shapes_still_disappear() {
    for source in [
        ": def\n  {% x\n  y %} z\n",
        "- a\n {% x\n y %}\n",
        "para\n# not a heading\n{% x\ny %}\n",
        "para\n- not an item\n{% x\ny %}\n",
        "p\n\na\n{% x\ny %} z\n",
        "- a\n\n  b\n  {% x\n  y %} z\n",
        "> p\n>\n> a\n> {% x\n> y %} z\n",
        "Hi. there\n{% x\ny %} z\n",
        "Iv. lead\n{% x\ny %} z\n",
        "a\n{% x\ny %} z\n",
        "- a\n  {% x\n  y %} z\n",
        "{% x %}\n",
        "{% x\n  y %}\n",
    ] {
        assert!(!djot_to_carve(source).contains('x'));
    }
    assert_eq!(
        to_html(&djot_to_carve("a {% x\ny %} z\n")).trim(),
        "<p>a  z</p>"
    );
}

#[test]
fn an_unindented_rejected_comment_ends_the_list() {
    let html = to_html(&djot_to_carve("- a\n{% x\ny %}\n"));
    assert!(html.contains("<li>a</li>"), "{html}");
    assert!(html.contains("<p>{% x\ny %}</p>"), "{html}");
}

#[test]
fn a_comment_in_a_footnote_body_is_inline() {
    let html = to_html(&djot_to_carve("a[^n]\n\n[^n]: text\n  {% x\n  y %} z\n"));
    assert!(!html.contains("{% x"), "{html}");
    assert!(html.contains("text") && html.contains('z'));
}

#[test]
fn consecutive_block_comments_do_not_recurse_through_their_context() {
    assert!(to_html(&djot_to_carve(&"{% x\n  y %}\n".repeat(64)))
        .trim()
        .is_empty());
}

#[test]
fn a_comment_that_fails_on_its_first_line_continues_the_heading() {
    let html = to_html(&djot_to_carve("# a\n{% 50% off %}\n"));
    assert!(html.contains("<h1>a {% 50% off %}</h1>"), "{html}");
    assert!(!html.contains("<p>"));
}

#[test]
fn shared_reference_fixtures_match_the_entire_html() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/djot-comment-boundary-fixtures.json")).unwrap();
    let normalize = |html: &str| {
        let between = regex::Regex::new(r">\s+<")
            .unwrap()
            .replace_all(html, "><")
            .into_owned();
        regex::Regex::new(r"\s+")
            .unwrap()
            .replace_all(&between, " ")
            .replace(" <", "<")
            .replace("> ", ">")
            .trim()
            .to_owned()
    };
    for fixture in fixtures.as_array().unwrap() {
        assert_eq!(
            normalize(&to_html(&djot_to_carve(
                fixture["source"].as_str().unwrap()
            ))),
            normalize(fixture["html"].as_str().unwrap()),
            "{}",
            fixture["id"]
        );
    }
}

#[test]
fn rejected_comments_preserve_later_blocks_and_quote_depth() {
    for (source, expected) in [
        (
            "> > inner\n> {% c %\n> tail\n",
            "> > inner\n>\n> \\{% c %\n> \\> tail\n",
        ),
        (
            "para {% open\n\n# Title {#keep}\n",
            "para \\{% open\n\n# Title \n",
        ),
        ("Done. > {% c %\n> next\n", "Done. > \\{% c %\n\\> next\n"),
        (
            "> text {% open\n>\n> next {#id}\n",
            "> text \\{% open\n>\n> next \n",
        ),
        ("Iv. > {% c %\n> next\n", "Iv. > \\{% c %\n\\> next\n"),
        ("did. > {% c %\n> next\n", "did. > \\{% c %\n> next\n"),
    ] {
        assert_eq!(djot_to_carve(source), expected, "{source:?}");
    }
}

#[test]
fn closing_comment_lines_and_crlf_blank_paragraphs() {
    assert!(!to_html(&djot_to_carve("> para {% a\n> %}\n")).contains("{%"));
    let source = djot_to_carve("{% a\r\n\r\nb %}\n");
    assert!(source.contains('a') && source.contains('b'), "{source}");
    assert_eq!(
        djot_to_carve("- > {% a\n > b %}\n"),
        "- > \\{% a\n > b %}\n"
    );
}

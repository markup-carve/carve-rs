use carve::{BlockNode, Options, OrderedListType};

fn roundtrip(source: &str, carrier: &str, style: OrderedListType, start: usize) {
    let markdown =
        carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    assert!(markdown.contains(carrier), "{source}: {markdown}");
    let imported = carve::migrate_markdown(&markdown);
    assert!(
        imported
            .report
            .diagnostics
            .iter()
            .all(|row| row.code != "carrier-markers-damaged" && row.code != "list-style-dropped"),
        "{markdown}: {:?}",
        imported.report
    );
    let document = carve::parse(&imported.value);
    let list = document
        .children
        .iter()
        .find_map(|node| match node {
            BlockNode::List(list) => Some(list),
            _ => None,
        })
        .unwrap();
    assert_eq!(list.ol_type, Some(style), "{}", imported.value);
    assert_eq!(list.start.unwrap_or(1), start, "{}", imported.value);
    assert_eq!(carve::to_html(&imported.value), carve::to_html(source));
}

#[test]
fn style_markers_restore_alpha_roman_and_ambiguous_lists() {
    roundtrip(
        "a.  one\na.  two\n",
        "<!-- carve: a. -->\n1. one",
        OrderedListType::LowerAlpha,
        1,
    );
    roundtrip(
        "iv.  one\nv.  two\n",
        "<!-- carve: iv. -->\n4. one",
        OrderedListType::LowerRoman,
        4,
    );
    roundtrip(
        "i.  one\nj.  two\n",
        "<!-- carve: i. j. -->\n9. one",
        OrderedListType::LowerAlpha,
        9,
    );
    roundtrip(
        "i.  one\nii.  two\n",
        "<!-- carve: i. ii. -->\n1. one",
        OrderedListType::LowerRoman,
        1,
    );
    roundtrip(
        "i.  one\n",
        "<!-- carve: i. -->\n1. one",
        OrderedListType::LowerRoman,
        1,
    );
}

#[test]
fn style_marker_must_be_adjacent_to_a_numeric_list() {
    let imported = carve::migrate_markdown("<!-- carve: a. -->\n\n1. one\n2. two\n");
    assert!(imported
        .report
        .diagnostics
        .iter()
        .any(|row| row.code == "carrier-markers-damaged"));
    let document = carve::parse(&imported.value);
    assert!(document
        .children
        .iter()
        .all(|node| !matches!(node, BlockNode::List(list) if list.ol_type.is_some())));
}

#[test]
fn style_marker_uses_markdown_start_and_delimiter() {
    let imported = carve::migrate_markdown("<!-- carve: a. -->\n4) one\n5) two\n");
    assert!(
        imported
            .report
            .diagnostics
            .iter()
            .all(|row| !row.code.starts_with("carrier-marker")),
        "{:?}",
        imported.report
    );
    let document = carve::parse(&imported.value);
    let BlockNode::List(list) = &document.children[0] else {
        panic!("{}", imported.value)
    };
    assert_eq!(list.ol_type, Some(OrderedListType::LowerAlpha));
    assert_eq!(list.start, Some(4));
    assert_eq!(list.delim, Some(')'));
}

#[test]
fn style_marker_survives_quote_and_attribute_hosts() {
    for (source, fragment) in [
        ("> a.  q\n> b.  r\n", "> <!-- carve: a. -->\n> 1. q"),
        (
            "{.steps}\na.  one\nb.  two\n",
            "<!-- carve: {.steps} -->\n<!-- carve: a. -->\n1. one",
        ),
    ] {
        let markdown =
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        assert!(markdown.contains(fragment), "{source}: {markdown}");
        let imported = carve::migrate_markdown(&markdown);
        assert!(
            imported
                .report
                .diagnostics
                .iter()
                .all(|row| !row.code.starts_with("carrier-marker")
                    && row.code != "list-style-dropped"),
            "{}: {:?}",
            imported.value,
            imported.report
        );
        assert_eq!(
            carve::to_html(&imported.value),
            carve::to_html(source),
            "{}",
            imported.value
        );
    }
}

#[test]
fn footnote_body_list_style_is_carried_only_at_eligible_depth() {
    let source = "[^n]: text\n\n    a.  one\n    b.  two\nafter [^n]\n";
    let markdown =
        carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    assert!(markdown.contains("<!-- carve: a. -->"), "{markdown}");
    let imported = carve::migrate_markdown(&markdown);
    assert!(
        imported
            .report
            .diagnostics
            .iter()
            .all(|row| !row.code.starts_with("carrier-marker") && row.code != "list-style-dropped"),
        "{}: {:?}",
        imported.value,
        imported.report
    );
    assert_eq!(carve::to_html(&imported.value), carve::to_html(source));
}

#[test]
fn nested_style_and_continuation_stay_in_their_lists() {
    for source in [
        "- outer\n\n  a.  first\n  b.  second\n",
        "a.  first\n    continued\nb.  second\n",
        "a.  one\n\nI.  two\n",
    ] {
        let markdown =
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        let imported = carve::migrate_markdown(&markdown);
        assert!(
            imported
                .report
                .diagnostics
                .iter()
                .all(|row| !row.code.starts_with("carrier-marker")),
            "{source}: {markdown}: {:?}",
            imported.report
        );
        assert_eq!(
            carve::to_html(&imported.value),
            carve::to_html(source),
            "{source}: {markdown} => {}",
            imported.value
        );
    }
}

#[test]
fn a_styled_list_before_a_plain_list_keeps_its_style() {
    let source = "a.  one\n\n1. two\n";
    let markdown =
        carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    let imported = carve::migrate_markdown(&markdown);
    let document = carve::parse(&imported.value);
    let BlockNode::List(first) = &document.children[0] else {
        panic!("{}", imported.value)
    };
    assert_eq!(first.ol_type, Some(OrderedListType::LowerAlpha));
}

#[test]
fn footnote_opening_and_nested_lists_keep_a_style_loss() {
    for source in [
        "[^n]: a.  one\n",
        "[^n]: text\n\n    - x\n\n      a.  one\n",
        "[^n]: text\n\n    > a.  one\n",
    ] {
        let markdown =
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        assert!(
            !markdown.contains("<!-- carve: a. -->"),
            "{source}: {markdown}"
        );
    }
}

#[test]
fn marker_on_a_list_item_opening_line_is_dropped() {
    let imported = carve::migrate_markdown("- <!-- carve: a. -->\n  1. one\n  2. two\n- next\n");
    assert!(
        imported
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "carrier-marker-dropped"),
        "{:?}",
        imported.report
    );
    assert!(imported.value.contains("- next"), "{}", imported.value);
}

#[test]
fn an_attribute_above_other_content_does_not_move_to_the_styled_list() {
    let markdown = "<!-- carve: {.steps} -->\nIntro.\n\n<!-- carve: a. -->\n1. one\n2. two\n";
    let imported = carve::migrate_markdown(markdown);
    assert!(
        imported
            .report
            .diagnostics
            .iter()
            .all(|row| !row.code.starts_with("carrier-marker")),
        "{:?}",
        imported.report
    );
    let document = carve::parse(&imported.value);
    let BlockNode::Paragraph(paragraph) = &document.children[0] else {
        panic!("{}", imported.value)
    };
    assert_eq!(paragraph.attrs.as_ref().unwrap().classes, ["steps"]);
    let BlockNode::List(list) = &document.children[1] else {
        panic!("{}", imported.value)
    };
    assert_eq!(list.ol_type, Some(OrderedListType::LowerAlpha));
    assert!(list.attrs.is_none());
}

#[test]
fn a_style_marker_cannot_fill_an_empty_footnote_opening() {
    let imported =
        carve::migrate_markdown("[^n]:\n    <!-- carve: a. -->\n    1. one\n\nref [^n]\n");
    assert!(
        imported
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "carrier-markers-damaged"),
        "{}: {:?}",
        imported.value,
        imported.report
    );
}

#[test]
fn adjacent_nested_styles_do_not_consume_each_others_tokens() {
    let source = "- outer\n\n  a.  one\n\n  ii.  two\n";
    let markdown =
        carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    let imported = carve::migrate_markdown(&markdown);
    assert!(
        !imported.value.contains("CARVECARRIER"),
        "{}",
        imported.value
    );
    assert_eq!(
        carve::to_html(&imported.value),
        carve::to_html(source),
        "{markdown} => {}",
        imported.value
    );
}

#[test]
fn style_carrier_on_an_item_opening_line_reports_loss() {
    let source = "- a.  one\n  b.  two\n";
    let result = carve::with_render_loss_report(
        carve::RenderTarget::Markdown,
        carve::CheckedRenderOptions::default(),
        || {
            carve::render_markdown_with_options(
                &carve::parse(source),
                &Options::default().with_roundtrip(true),
            )
            .unwrap()
        },
    )
    .unwrap();
    assert!(
        !result.value.contains("<!-- carve: a. -->"),
        "{}",
        result.value
    );
    assert!(result
        .losses
        .iter()
        .any(|loss| loss.code == "list-style-dropped"));
}

#[test]
fn nested_style_token_does_not_end_the_outer_styled_list() {
    let source = "- host\n\n  i.  outer one\n\n      a.  inner one\n      b.  inner two\n  ii.  outer two\n  iii.  outer three\n";
    let markdown =
        carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    let imported = carve::migrate_markdown(&markdown);
    assert!(
        !imported.value.contains("CARVECARRIER"),
        "{}",
        imported.value
    );
    assert_eq!(
        carve::to_html(&imported.value),
        carve::to_html(source),
        "{markdown} => {}",
        imported.value
    );
}

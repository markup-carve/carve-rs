#[test]
fn a_caption_below_an_items_content_column_stays_paragraph_text() {
    let source = "- {#x}\n  ![two](b.png)\n^ (b) Two\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><p id=\"x\"><img src=\"b.png\" alt=\"two\">\n^ (b) Two</p></li>\n</ul>"
    );
}

#[test]
fn a_caption_at_an_items_content_column_still_attaches() {
    let source = "- {#x}\n  ![two](b.png)\n  ^ (b) Two\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>\n    <figure id=\"x\">\n      <img src=\"b.png\" alt=\"two\">\n      <figcaption>(b) Two</figcaption>\n    </figure>\n  </li>\n</ul>"
    );
}

#[test]
fn a_one_column_shift_stays_below_the_items_content_column() {
    let source = "- {#x}\n  ![red](b.png)\n ^ Second panel\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><p id=\"x\"><img src=\"b.png\" alt=\"red\">\n^ Second panel</p></li>\n</ul>"
    );
}

#[test]
fn a_quoted_item_keeps_its_below_column_caption_as_text() {
    let source = "> - {#x}\n>   ![red](b.png)\n> ^ Second panel\n";
    assert_eq!(
        carve::to_html(source),
        "<blockquote>\n  <ul>\n    <li><p id=\"x\"><img src=\"b.png\" alt=\"red\">\n^ Second panel</p></li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn a_caption_after_a_plain_quote_still_attaches() {
    assert_eq!(
        carve::to_html("> Stay hungry\n^ Steve Jobs\n"),
        "<figure>\n  <blockquote><p>Stay hungry</p></blockquote>\n  <figcaption>Steve Jobs</figcaption>\n</figure>"
    );
}

#[test]
fn a_caption_after_a_plus_attached_image_still_attaches() {
    let source = "- x\n+\n![a](i.png)\n^ cap\n+\n![b](j.png)\n^ next\n";
    assert_eq!(carve::to_html(source).matches("<figure>").count(), 2);
}

#[test]
fn a_folded_caret_line_keeps_its_authored_position() {
    let source = "- [ ] - [ ] :: t\n      ^ ap\n     ^ cap\n";
    let json = carve::to_json_with_options(source, &carve::Options::default().with_positions(true));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut found = false;
    fn check(value: &serde_json::Value, source: &str, found: &mut bool) {
        if value.get("value").and_then(|v| v.as_str()) == Some("^ cap") {
            let pos = &value["pos"];
            let start = pos["startOffset"].as_u64().unwrap() as usize;
            let end = pos["endOffset"].as_u64().unwrap() as usize;
            let slice: String = source.chars().skip(start).take(end - start).collect();
            assert_eq!(slice, "^ cap");
            *found = true;
        }
        match value {
            serde_json::Value::Array(items) => {
                for item in items {
                    check(item, source, found);
                }
            }
            serde_json::Value::Object(fields) => {
                for child in fields.values() {
                    check(child, source, found);
                }
            }
            _ => {}
        }
    }
    check(&value, source, &mut found);
    assert!(found);
}

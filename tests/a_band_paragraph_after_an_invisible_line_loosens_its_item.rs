use serde_json::Value;

#[test]
fn band_followers_obey_the_comments_content_column() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/band_comment_columns_wave.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let document = carve::parse(source);
        let tree: Value = serde_json::from_str(&carve::to_json(&document)).unwrap();
        let list = &tree["children"][0];
        assert_eq!(list["tight"], row["tight"], "{source}");
        let kinds: Vec<_> = list["items"][0]["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["type"].clone())
            .collect();
        assert_eq!(kinds, *row["itemKinds"].as_array().unwrap(), "{source}");
        assert_eq!(
            carve::to_html(source),
            row["html"].as_str().unwrap(),
            "{source}"
        );
    }
}

#[test]
fn a_blank_before_an_invisible_comment_still_loosens_the_next_paragraph() {
    let source = "- item\n\n  :: c\n\n    %% note\n    more\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><p>item</p>\n    <dl>\n      <dt>c</dt>\n    </dl>\n    <p>more</p>\n  </li>\n</ul>"
    );
}

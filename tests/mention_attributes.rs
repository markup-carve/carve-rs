fn doc_json(node_type: &str, name_key: &str, name: &str, attrs: &str) -> String {
    let attrs = if attrs.is_empty() {
        String::new()
    } else {
        format!(",\"attrs\":{attrs}")
    };
    format!(
        "{{\"type\":\"document\",\"children\":[{{\"type\":\"paragraph\",\"children\":[{{\"type\":\"{node_type}\"{attrs},\"{name_key}\":\"{name}\"}}]}}],\"srcByteLength\":0}}"
    )
}

fn render_node(node_type: &str, name_key: &str, name: &str, attrs: &str) -> String {
    let doc = carve::from_json(&doc_json(node_type, name_key, name, attrs)).expect("decode node");
    carve::render_html(&doc).expect("a one-node tree is within the render ceiling")
}

fn render_node_with_links(node_type: &str, name_key: &str, name: &str, attrs: &str) -> String {
    let doc = carve::from_json(&doc_json(node_type, name_key, name, attrs)).expect("decode node");
    let options = carve::Options::new()
        .with_mention_url("/u/{name}")
        .with_tag_url("/t/{name}");
    carve::render_html_with_options(&doc, &options)
        .expect("a one-node tree is within the render ceiling")
}

#[test]
fn mention_attrs_render_on_span_form() {
    assert_eq!(
        render_node("mention", "user", "alice", ""),
        "<p><span class=\"mention\"><strong>@alice</strong></span></p>"
    );
    assert_eq!(
        render_node(
            "mention",
            "user",
            "alice",
            "{\"id\":\"x\",\"order\":[\"#id\"]}"
        ),
        "<p><span class=\"mention\" id=\"x\"><strong>@alice</strong></span></p>"
    );
    assert_eq!(
        render_node(
            "mention",
            "user",
            "alice",
            "{\"classes\":[\"user\"],\"order\":[\".class\"]}"
        ),
        "<p><span class=\"mention user\"><strong>@alice</strong></span></p>"
    );
    assert_eq!(
        render_node(
            "mention",
            "user",
            "alice",
            "{\"id\":\"x\",\"classes\":[\"user\"],\"keyValues\":{\"data-role\":\"lead\"},\"order\":[\"data-role\",\".class\",\"#id\"]}"
        ),
        "<p><span class=\"mention user\" data-role=\"lead\" id=\"x\"><strong>@alice</strong></span></p>"
    );
}

#[test]
fn mention_attrs_render_on_link_form() {
    assert_eq!(
        render_node_with_links("mention", "user", "alice", ""),
        "<p><a class=\"mention\" href=\"/u/alice\">@alice</a></p>"
    );
    assert_eq!(
        render_node_with_links(
            "mention",
            "user",
            "alice",
            "{\"id\":\"x\",\"order\":[\"#id\"]}"
        ),
        "<p><a class=\"mention\" href=\"/u/alice\" id=\"x\">@alice</a></p>"
    );
    assert_eq!(
        render_node_with_links(
            "mention",
            "user",
            "alice",
            "{\"classes\":[\"user\"],\"order\":[\".class\"]}"
        ),
        "<p><a class=\"mention user\" href=\"/u/alice\">@alice</a></p>"
    );
    assert_eq!(
        render_node_with_links(
            "mention",
            "user",
            "alice",
            "{\"id\":\"x\",\"classes\":[\"user\"],\"keyValues\":{\"data-role\":\"lead\"},\"order\":[\"data-role\",\".class\",\"#id\"]}"
        ),
        "<p><a class=\"mention user\" href=\"/u/alice\" data-role=\"lead\" id=\"x\">@alice</a></p>"
    );
    assert_eq!(
        render_node_with_links(
            "mention",
            "user",
            "alice",
            "{\"keyValues\":{\"href\":\"/evil\"},\"order\":[\"href\"]}"
        ),
        "<p><a class=\"mention\" href=\"/u/alice\">@alice</a></p>"
    );
}

#[test]
fn tag_attrs_render_on_span_form() {
    assert_eq!(
        render_node("tag", "name", "release", ""),
        "<p><span class=\"tag\"><strong>#release</strong></span></p>"
    );
    assert_eq!(
        render_node(
            "tag",
            "name",
            "release",
            "{\"id\":\"x\",\"order\":[\"#id\"]}"
        ),
        "<p><span class=\"tag\" id=\"x\"><strong>#release</strong></span></p>"
    );
    assert_eq!(
        render_node(
            "tag",
            "name",
            "release",
            "{\"classes\":[\"user\"],\"order\":[\".class\"]}"
        ),
        "<p><span class=\"tag user\"><strong>#release</strong></span></p>"
    );
    assert_eq!(
        render_node(
            "tag",
            "name",
            "release",
            "{\"id\":\"x\",\"classes\":[\"user\"],\"keyValues\":{\"data-role\":\"lead\"},\"order\":[\"data-role\",\".class\",\"#id\"]}"
        ),
        "<p><span class=\"tag user\" data-role=\"lead\" id=\"x\"><strong>#release</strong></span></p>"
    );
}

#[test]
fn tag_attrs_render_on_link_form() {
    assert_eq!(
        render_node_with_links("tag", "name", "release", ""),
        "<p><a class=\"tag\" href=\"/t/release\">#release</a></p>"
    );
    assert_eq!(
        render_node_with_links(
            "tag",
            "name",
            "release",
            "{\"id\":\"x\",\"order\":[\"#id\"]}"
        ),
        "<p><a class=\"tag\" href=\"/t/release\" id=\"x\">#release</a></p>"
    );
    assert_eq!(
        render_node_with_links(
            "tag",
            "name",
            "release",
            "{\"classes\":[\"user\"],\"order\":[\".class\"]}"
        ),
        "<p><a class=\"tag user\" href=\"/t/release\">#release</a></p>"
    );
    assert_eq!(
        render_node_with_links(
            "tag",
            "name",
            "release",
            "{\"id\":\"x\",\"classes\":[\"user\"],\"keyValues\":{\"data-role\":\"lead\"},\"order\":[\"data-role\",\".class\",\"#id\"]}"
        ),
        "<p><a class=\"tag user\" href=\"/t/release\" data-role=\"lead\" id=\"x\">#release</a></p>"
    );
    assert_eq!(
        render_node_with_links(
            "tag",
            "name",
            "release",
            "{\"keyValues\":{\"href\":\"/evil\"},\"order\":[\"href\"]}"
        ),
        "<p><a class=\"tag\" href=\"/t/release\">#release</a></p>"
    );
}

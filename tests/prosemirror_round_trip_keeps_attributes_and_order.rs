use carve::{
    from_json, from_prosemirror, parse_with_options, render_carve, render_html, to_json,
    to_prosemirror, Options,
};

fn round_trip(source: &str) -> (String, String, bool) {
    let original = parse_with_options(source, &Options::default().with_positions(true));
    let pm = to_prosemirror(&original);
    let strict = pm.dropped.is_empty() && pm.degraded.is_empty();
    let returned = from_prosemirror(&pm.json).expect("the bridge output reads back");
    (
        render_html(&original).unwrap(),
        render_html(&returned).unwrap(),
        strict,
    )
}

fn assert_same_html(source: &str) {
    let (before, after, strict) = round_trip(source);
    assert!(strict, "{source:?} reported a loss");
    assert_eq!(after, before, "{source:?}");
}

#[test]
fn a_block_image_keeps_its_block_attributes() {
    assert_same_html("{.outer k=one}\n![a](b){.inner k=two}\n");
    assert_same_html("{#i}\n![a](b)\n");
}

#[test]
fn a_block_image_with_a_comment_on_its_line_stays_a_block_image() {
    assert_same_html("![a](/i) %% c\n^ cap\n");
    assert_same_html("![a](/i) {% c %}\n");
}

#[test]
fn a_combined_token_carries_its_attributes_on_the_outer_element_only() {
    assert_same_html("/*a*/{.q}\n");
    assert_same_html("x */b/*{.k #i} y\n");
    let source = "{/a */b/*{.k} c/}\n";
    assert_same_html(source);
    let original = parse_with_options(source, &Options::default());
    let returned = from_prosemirror(&to_prosemirror(&original).json).unwrap();
    assert_eq!(
        render_carve(&returned).unwrap(),
        render_carve(&original).unwrap()
    );
}

#[test]
fn a_strong_run_beside_a_combined_token_is_reported_as_merging() {
    for source in ["*a*/*b*/\n", "/*a*/*b*\n"] {
        let original = parse_with_options(source, &Options::default());
        let pm = to_prosemirror(&original);
        assert!(
            !pm.degraded.is_empty(),
            "{source:?} merges two marks of one kind without a report"
        );
    }
}

#[test]
fn an_empty_class_token_survives() {
    assert_same_html("{class=\"\"}\npara\n");
}

#[test]
fn a_key_named_like_a_bridge_field_survives() {
    for source in [
        "[a]{href=javascript:x}\n",
        "[a]{src=x level=2 name=n}\n",
        "{label=l content=c}\npara\n",
        "[safe](https://example.com){href=javascript:steal}\n",
    ] {
        assert_same_html(source);
        let original = parse_with_options(source, &Options::default().with_positions(true));
        let returned = from_prosemirror(&to_prosemirror(&original).json).unwrap();
        assert_eq!(
            carve::to_carve(&render_carve(&returned).unwrap()),
            carve::to_carve(source),
            "{source:?}"
        );
    }
}

#[test]
fn notes_come_back_in_definition_order() {
    let source = "[^f] [^g]\n\n[^f]: [^g]: a\n    # H\n";
    let original = parse_with_options(source, &Options::default().with_positions(true));
    let returned = from_prosemirror(&to_prosemirror(&original).json).unwrap();
    assert_eq!(
        carve::to_carve(&render_carve(&returned).unwrap()),
        carve::to_carve(source)
    );
}

#[test]
fn a_key_that_already_spells_the_escape_prefix_survives() {
    let ast = r#"{"type":"document","children":[{"type":"paragraph","children":[{"type":"text","value":"a"}],"attrs":{"keyValues":{"carveAttr:href":"x","href":"y"},"order":["carveAttr:href","href"]}}],"srcByteLength":0}"#;
    let doc = from_json(ast).expect("the AST payload is valid");
    let returned = from_prosemirror(&to_prosemirror(&doc).json).unwrap();
    assert_eq!(render_html(&returned).unwrap(), render_html(&doc).unwrap());
    let json = to_json(&returned);
    assert!(
        json.contains(r#""carveAttr:href":"x""#) && json.contains(r#""href":"y""#),
        "{json}"
    );
}

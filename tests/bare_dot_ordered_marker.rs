//! Bare-dot ordered markers: `. item` is decimal ordered shorthand, but the
//! authored spelling is runtime-only state and is not published in PART 12 JSON.

fn html(src: &str) -> String {
    carve::to_html(src)
}

fn first_list(source: &str) -> carve::List {
    let mut doc = carve::parse(source);
    let carve::BlockNode::List(list) = doc.children.remove(0) else {
        panic!("first block is not a list");
    };
    list
}

#[test]
fn renders_decimal_ordered_list_from_one_without_attrs_on_ol() {
    assert_eq!(
        html(". first\n. second\n"),
        "<ol>\n  <li>first</li>\n  <li>second</li>\n</ol>"
    );
}

#[test]
fn mixes_with_explicit_decimal_dot_in_one_list() {
    let expected = "<ol>\n  <li>a</li>\n  <li>b</li>\n</ol>";
    assert_eq!(html(". a\n2. b\n"), expected);
    assert_eq!(html("1. a\n. b\n"), expected);
}

#[test]
fn delimiter_change_starts_a_sibling_list() {
    assert_eq!(
        html(". a\n1) b\n"),
        "<ol>\n  <li>a</li>\n</ol>\n<ol>\n  <li>b</li>\n</ol>"
    );
}

#[test]
fn needs_space_and_non_empty_content() {
    assert_eq!(html("."), "<p>.</p>");
    assert_eq!(html(".   "), "<p>.</p>");
    assert_eq!(html(".x"), "<p>.x</p>");
    assert_eq!(html(".. text"), "<p>.. text</p>");
}

#[test]
fn attributes_attach_before_required_space() {
    assert_eq!(
        html(".{#x .k} text"),
        "<ol>\n  <li id=\"x\" class=\"k\">text</li>\n</ol>"
    );
    assert_eq!(html(".{k=v}text"), "<p>.{k=v}text</p>");
}

#[test]
fn does_not_interrupt_an_open_paragraph() {
    assert_eq!(html("text\n. item"), "<p>text\n. item</p>");
}

#[test]
fn runtime_field_is_set_only_from_bare_opened_list() {
    assert!(first_list(". a\n").bare_marker);
    assert!(!first_list("1. a\n").bare_marker);
}

#[test]
fn serialized_ast_does_not_publish_runtime_field() {
    let json = carve::to_json(&carve::parse(". a\n"));
    assert!(!json.contains("bareMarker"), "{json}");
    assert!(!json.contains("bare_marker"), "{json}");

    let decoded = carve::from_json(&json).expect("decode json");
    let carve::BlockNode::List(list) = &decoded.children[0] else {
        panic!("decoded first block is not a list");
    };
    assert!(!list.bare_marker);
    assert_eq!(carve::render_carve(&decoded), "1. a\n");
}

#[test]
fn writer_preserves_the_opener_spelling() {
    assert_eq!(carve::to_carve(". a\n. b\n"), ". a\n. b\n");
    assert_eq!(carve::to_carve("1. a\n2. b\n"), "1. a\n2. b\n");
    assert_eq!(carve::to_carve(". a\n2. b\n"), ". a\n. b\n");
    assert_eq!(carve::to_carve("1. a\n. b\n"), "1. a\n2. b\n");
}

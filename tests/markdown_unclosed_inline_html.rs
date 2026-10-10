//! An unknown inline tag becomes a raw span holding that tag and nothing more,
//! so the text after its `>` stays in the document
//! (markup-carve/carve-rs#2409).

use carve::markdown_to_carve;

#[test]
fn an_unknown_tag_in_a_list_item_stops_at_its_own_closing_bracket() {
    assert_eq!(
        markdown_to_carve("- one <Widget> two\n- three\n- four\n"),
        "- one `<Widget>`{=html} two\n- three\n- four\n"
    );
}

#[test]
fn an_unknown_tag_in_a_paragraph_stops_at_its_own_closing_bracket() {
    assert_eq!(
        markdown_to_carve("before <Widget> after\n\n## Heading\n\n- a\n- b\n"),
        "before `<Widget>`{=html} after\n\n## Heading\n\n- a\n- b\n"
    );
}

#[test]
fn an_unclosed_native_tag_does_not_drop_later_items() {
    assert_eq!(
        markdown_to_carve("- one <b>two\n- three\n- four\n"),
        "- one `<b>`{=html}two\n- three\n- four\n"
    );
}

#[test]
fn paired_and_self_closing_unknown_tags_keep_their_behavior() {
    assert_eq!(
        markdown_to_carve("- one <Widget>x</Widget> two\n"),
        "- one `<Widget>`{=html}x`</Widget>`{=html} two\n"
    );
    assert_eq!(
        markdown_to_carve("- one <Widget/> two\n"),
        "- one `<Widget/>`{=html} two\n"
    );
}

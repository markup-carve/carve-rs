//! A resolved Markdown shortcut reference imports as a collapsed reference and
//! keeps its definition; text that cannot be a label falls back to an inline
//! link (markup-carve/carve#3241).

#[test]
fn a_shortcut_reference_keeps_its_definition() {
    for (source, expected) in [
        ("[foo]\n\n[foo]: /u", "[foo][]\n\n[foo]: /u"),
        (
            "[foo] and [bar][]\n\n[foo]: /u\n[bar]: /v",
            "[foo][] and [bar][]\n\n[foo]: /u\n\n[bar]: /v",
        ),
        ("[Foo]\n\n[foo]: /u \"T\"", "[Foo][foo]\n\n[foo]: /u \"T\""),
        ("[foo] [foo][]\n\n[foo]: /u", "[foo][] [foo][]\n\n[foo]: /u"),
    ] {
        let written = carve::try_markdown_to_carve(source).expect(source);
        assert_eq!(written.trim_end(), expected, "{source:?}");
    }
}

#[test]
fn a_shortcut_whose_text_is_no_label_is_inlined() {
    for (source, expected) in [
        ("[*foo*]\n\n[*foo*]: /u", "[/foo/](/u)"),
        ("[a\nb]\n\n[a b]: /u", "[a\nb](/u)"),
    ] {
        let written = carve::try_markdown_to_carve(source).expect(source);
        assert_eq!(written.trim_end(), expected, "{source:?}");
    }
}

#[test]
fn a_collapsed_import_renders_like_the_shortcut() {
    let written = carve::try_markdown_to_carve("[foo]\n\n[foo]: /u \"T\"").unwrap();
    assert_eq!(
        carve::to_html(&written).trim_end(),
        "<p><a href=\"/u\" title=\"T\">foo</a></p>"
    );
}

//! A Markdown definition whose title wraps has no Carve definition spelling,
//! so its uses import as inline links (markup-carve/carve-rs#2822).

#[test]
fn a_definition_whose_title_wraps_is_inlined() {
    for (source, expected) in [
        ("[t][r]\n\n[r]: /u \"x\ny\"", "[t](/u \"x\ny\")"),
        ("[t][r]\n\n[r]: /u\n  'x\ny'", "[t](/u \"x\ny\")"),
        (
            "[a][r] and [b][r]\n\n[r]: /u (x\ny)",
            "[a](/u \"x\ny\") and [b](/u \"x\ny\")",
        ),
    ] {
        let written = carve::try_markdown_to_carve(source).expect(source);
        assert_eq!(written.trim_end(), expected, "{source:?}");
        assert_eq!(
            carve::to_html(&written).trim_end(),
            format!(
                "<p>{}</p>",
                expected
                    .replace("[t](/u \"x\ny\")", "<a href=\"/u\" title=\"x\ny\">t</a>")
                    .replace("[a](/u \"x\ny\")", "<a href=\"/u\" title=\"x\ny\">a</a>")
                    .replace("[b](/u \"x\ny\")", "<a href=\"/u\" title=\"x\ny\">b</a>")
            ),
            "{source:?}"
        );
    }
}

#[test]
fn a_definition_with_a_one_line_title_stays_a_definition() {
    let written = carve::try_markdown_to_carve("[t][r]\n\n[r]: /u \"x y\"").unwrap();
    assert_eq!(written.trim_end(), "[t][r]\n\n[r]: /u \"x y\"");
}

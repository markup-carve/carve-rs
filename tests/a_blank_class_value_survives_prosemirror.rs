use carve::{from_prosemirror, parse_with_options, render_html, to_prosemirror, Options};

#[test]
fn a_whitespace_only_class_value_survives_prosemirror() {
    for source in [
        "{class=\" \"}\npara\n",
        "{class=\"  \"}\npara\n",
        "[a]{class=\" \"}\n",
        "{class=\"\"}\npara\n",
    ] {
        let original = parse_with_options(source, &Options::default().with_positions(true));
        let returned = from_prosemirror(&to_prosemirror(&original).json).unwrap();
        assert_eq!(
            render_html(&returned).unwrap(),
            render_html(&original).unwrap(),
            "{source:?}"
        );
    }
}

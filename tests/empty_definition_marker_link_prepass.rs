#[test]
fn empty_markers_do_not_open_a_link_definition_prepass_context() {
    for source in [
        ":: \n: [f]:\n",
        "::\n: [f]:\n",
        ": \n: [f]:\n",
        ":: \t\n: [f]:\n",
    ] {
        assert!(
            carve::to_html(source).contains("[f]:"),
            "lost the visible reference-shaped text in {source:?}"
        );
    }
    assert_eq!(carve::to_html(":: \n: [f]:\n"), "<p>::\n: [f]:</p>");
}

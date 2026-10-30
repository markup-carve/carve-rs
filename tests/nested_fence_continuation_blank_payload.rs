use carve::{to_html, to_html_with_options, Options};

#[test]
fn indented_plus_after_nested_fence_matches_its_column() {
    for source in [
        "- - ```\n\n\n  +\ntail\n",
        "- - ```\n\n  +\n> q\n",
        "- - ```\n\n  +\n- s\n",
        "- - ```\n\n  +\n~~~\ny\n",
        "- 1. ```\n\n\n  +\ntail\n",
        "- - - ```\n\n\n    +\ntail\n",
        "- - [ ] ```\n\n\n  +\ntail\n",
    ] {
        super::a_continuation_marker_is_column_zero_only::assert_legacy_model(
            source,
            &to_html(source),
        );
        super::a_continuation_marker_is_column_zero_only::assert_legacy_model(
            source,
            &to_html_with_options(source, &Options::default().with_positions(true)),
        );
    }
}

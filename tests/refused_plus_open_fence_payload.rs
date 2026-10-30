use carve::to_html;

#[test]
#[ignore = "carve#4409: a line after a letter below a nested fence, listed on carve-rs#3589"]
fn column_zero_markers_and_indented_plus_below_fences_follow_their_columns() {
    for source in [
        "- ```\n+\n\ntail",
        "- ```\n  x\n+\n\ntail",
        "- - ```\n  +\n\ntail",
        "- - ```\n    x\n  +\n\ntail",
        "- - - ```\n    +\n\ntail",
        "10. 10. ```\n    +\n\ntail",
        "- [ ] - [ ] ```\n      +\n\ntail",
        "- [ ] - [ ] - [ ] ```\n            +\n\n",
        "- - ```=html\n  +\n\ntail",
    ] {
        super::a_continuation_marker_is_column_zero_only::assert_legacy_model(
            source,
            &to_html(source),
        );
    }
}

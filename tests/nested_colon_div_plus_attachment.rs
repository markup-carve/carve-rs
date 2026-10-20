use carve::to_html;

#[test]
fn nested_colon_div_ends_before_attached_tail() {
    for source in ["- - ::: d\n  +\ntail", "- - - ::: d\n    +\ntail"] {
        super::a_continuation_marker_is_column_zero_only::assert_legacy_model(
            source,
            &to_html(source),
        );
    }
}

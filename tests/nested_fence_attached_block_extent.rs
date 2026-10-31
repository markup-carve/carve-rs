use carve::to_html;

#[test]
fn closed_fence_after_nested_marker_stays_a_document_block() {
    let source = "- - ```\n  +\n~~~\ny\n~~~\ntail";
    super::a_continuation_marker_is_column_zero_only::assert_legacy_model(source, &to_html(source));
}

#[test]
fn comment_before_attached_paragraph_keeps_paragraph_in_item() {
    let source = "- - ```\n  +\n%% c\ntail";
    super::a_continuation_marker_is_column_zero_only::assert_legacy_model(source, &to_html(source));
}

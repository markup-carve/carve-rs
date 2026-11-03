#[test]
fn a_quote_below_an_empty_nested_item_uses_one_column() {
    crate::common::assert_rule_a_pins("a_block_below_an_empty_nested_item_uses_one_column::a_quote_below_an_empty_nested_item_uses_one_column");
}

#[test]
fn a_code_fence_below_an_empty_nested_item_uses_one_column() {
    crate::common::assert_rule_a_pins("a_block_below_an_empty_nested_item_uses_one_column::a_code_fence_below_an_empty_nested_item_uses_one_column");
}

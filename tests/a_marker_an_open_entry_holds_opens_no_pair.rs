//! A marker an open entry of its kind holds never opens a pair, and a `/*` pairs
//! with its combined closer first, so a block behind such a marker cannot hide
//! a closer (PART 9 §9 R2, R3, R4, THE COMBINED TOKEN; markup-carve/carve#3960).

#[track_caller]
fn html(source: &str, expected: &str) {
    assert_eq!(carve::to_html(source), expected, "{source:?}");
}

#[test]
fn a_marker_that_closes_its_pair_takes_no_later_closer_as_its_own() {
    html("{=_*_*_{c==}}", "<p><mark><u>*</u>*_{c=</mark>}</p>");
    html(
        "_x /*a*/{.} b/{c_} c_",
        "<p><u>x <strong><em>a</em></strong>{.} b/{c</u>} c_</p>",
    );
    html("{=_/_/_{c==}}", "<p><mark><u>/</u>/_{c=</mark>}</p>");
}

#[test]
fn a_combined_token_takes_the_block_behind_its_closer() {
    html(
        "{~/*/x/*/{t=\"~\"}",
        "<p>{~<strong t=\"~\"><em>/x/</em></strong></p>",
    );
}

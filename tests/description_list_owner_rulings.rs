fn assert_html_in_both_position_modes(source: &str, expected: &str) {
    for positions in [false, true] {
        let options = carve::Options {
            positions,
            ..Default::default()
        };
        assert_eq!(
            carve::to_html_with_options(source, &options),
            expected,
            "positions={positions}"
        );
    }
}

#[test]
fn a_list_below_the_inner_description_belongs_to_the_outer_one() {
    let source = ":: T\n: D\n  :: t\n  :  d\n    - b\n";
    let expected = "<dl>\n  <dt>T</dt>\n  <dd>\n    <p>D</p>\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>b</li>\n    </ul>\n  </dd>\n</dl>";
    assert_html_in_both_position_modes(source, expected);
}

#[test]
fn a_term_past_the_description_column_interrupts_a_folded_list_marker() {
    let source = ":: t\n:  d\n    - b\n    :: u\n    :  e\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>d\n- b</p>\n    <dl>\n      <dt>u</dt>\n      <dd>e</dd>\n    </dl>\n  </dd>\n</dl>";
    assert_html_in_both_position_modes(source, expected);
}

#[test]
fn a_term_below_an_item_column_opens_in_the_surviving_description() {
    let source = "- h0\n  :: t\n  : d\n    [^n]: note\n    - a\n     :: u\n     :  e\n";
    let expected = "<ul>\n  <li>h0\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <p>d</p>\n        <dl>\n          <dt>u</dt>\n          <dd>e</dd>\n        </dl>\n      </dd>\n    </dl>\n  </li>\n</ul>";
    assert_html_in_both_position_modes(source, expected);
}

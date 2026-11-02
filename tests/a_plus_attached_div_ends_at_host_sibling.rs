//! A later colon closer cannot make a `+`-attached div own its host's sibling
//! marker (CARVE-P9-084; markup-carve/carve#4717).

#[test]
fn attached_div_leaves_a_sibling_and_a_later_closer_outside() {
    assert_eq!(
        carve::to_html("- +\n:::\n- b\n:::\n"),
        "<ul>\n  <li>\n    <div>\n\n    </div>\n  </li>\n  <li>b</li>\n</ul>\n<div>\n\n</div>"
    );
}

#[test]
fn a_code_fence_inside_the_div_keeps_a_marker_shaped_line() {
    assert_eq!(
        carve::to_html("- +\n:::\n```\n- b\n```\n- c\n:::\n"),
        "<ul>\n  <li>\n    <div>\n      <pre><code>- b\n</code></pre>\n    </div>\n  </li>\n  <li>c</li>\n</ul>\n<div>\n\n</div>"
    );
}

#[test]
fn an_unclosed_attached_div_stops_at_its_first_blank() {
    assert_eq!(
        carve::to_html("- +\n:::\nx\n\n y\n- b\n"),
        "<ul>\n  <li>\n    <div>\n      <p>x</p>\n    </div>\n  </li>\n</ul>\n<p>y\n- b</p>"
    );
}

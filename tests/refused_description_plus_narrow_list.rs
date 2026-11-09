#[test]
fn a_refused_description_plus_keeps_a_follower_in_a_narrow_item() {
    for (marker, list) in [("-", "ul"), ("*", "ul"), (".", "ol")] {
        let source = format!(":: t\n: +\n{marker} li\n+\n  + sib\n");
        let expected = format!(
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <{list}>\n      <li>li\n        + sib\n      </li>\n    </{list}>\n  </dd>\n</dl>"
        );
        assert_eq!(carve::to_html(&source), expected, "{source:?}");
    }
}

#[test]
fn a_wide_ordered_item_gives_a_different_list_to_its_description() {
    for pad in ["  ", "   "] {
        let source = format!(":: t\n: +\n1. li\n+\n{pad}- sib\n");
        assert_eq!(
            carve::to_html(&source),
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol>\n      <li>li</li>\n    </ol>\n    <ul>\n      <li>sib</li>\n    </ul>\n  </dd>\n</dl>",
            "{source:?}"
        );
    }
}

#[test]
fn a_bare_ordered_item_keeps_its_nested_different_list() {
    let source = ":: t\n: +\n. li\n+\n  - sib\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol>\n      <li>li\n        <ul>\n          <li>sib</li>\n        </ul>\n      </li>\n    </ol>\n  </dd>\n</dl>"
    );
}

#[test]
fn a_changed_ordered_delimiter_starts_a_description_sibling_list() {
    let source = ":: t\n: +\n1. li\n+\n  1) sib\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol>\n      <li>li</li>\n    </ol>\n    <ol data-delim=\")\">\n      <li>sib</li>\n    </ol>\n  </dd>\n</dl>"
    );
}

#[test]
fn a_refused_marker_between_list_siblings_does_not_loosen_the_list() {
    let source = ":: t\n: +\n- li\n+\n- sib\n";
    assert_eq!(
        carve::to_html(source),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>li</li>\n      <li>sib</li>\n    </ul>\n  </dd>\n</dl>"
    );
}

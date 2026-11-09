fn assert_html(source: &str, expected: &str) {
    assert_eq!(
        carve::render_html(&carve::parse(source)).unwrap(),
        expected,
        "parse/render: {source:?}"
    );
    assert_eq!(carve::to_html(source), expected, "to_html: {source:?}");
}

#[test]
fn a_marker_after_a_refused_plus_uses_the_attached_lists_boundary() {
    assert_html(
        ":: t\n: +\n1. li\n+\n - sib\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol>\n      <li>li</li>\n    </ol>\n  </dd>\n</dl>\n<ul>\n  <li>sib</li>\n</ul>",
    );
    assert_html(
        ":: t\n: +\n1. li\n+\n   1. sib\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol>\n      <li>li</li>\n      <li>sib</li>\n    </ol>\n  </dd>\n</dl>",
    );
    assert_html(
        "> :: t\n> : +\n> 1. li\n> +\n>   1. sib\n",
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <ol>\n        <li>li</li>\n        <li>sib</li>\n      </ol>\n    </dd>\n  </dl>\n</blockquote>",
    );
    assert_html(
        ":: t\n: +\n1. li\n+\n 1. sib\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol>\n      <li>li</li>\n      <li>sib</li>\n    </ol>\n  </dd>\n</dl>",
    );
    assert_html(
        "> :: t\n> : +\n> 1. li\n> +\n>  - sib\n",
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <ol>\n        <li>li</li>\n      </ol>\n    </dd>\n  </dl>\n  <ul>\n    <li>sib</li>\n  </ul>\n</blockquote>",
    );
    assert_html(
        ":: t\n: +\n- li\n+\n  - sib\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>li\n        <ul>\n          <li>sib</li>\n        </ul>\n      </li>\n    </ul>\n  </dd>\n</dl>",
    );
}

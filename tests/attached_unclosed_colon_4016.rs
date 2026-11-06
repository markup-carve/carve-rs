#[test]
fn an_unclosed_admonition_keeps_its_postblank_paragraph_and_lazy_line() {
    let source = "- a\n  - +\n  :::: note\n\n    p\ntail\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <aside class=\"admonition note\" aria-label=\"Note\">\n          <p>p\ntail</p>\n        </aside>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn an_unclosed_line_block_keeps_its_postblank_stanza() {
    let source = "- a\n  - +\n  ::: |\n\n    p\ntail\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>p<br>\ntail</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_sibling_after_the_unclosed_colon_block_stays_outside() {
    let source = "- a\n  - +\n  :::: note\n\n    p\ntail\n- next\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <aside class=\"admonition note\" aria-label=\"Note\">\n          <p>p\ntail</p>\n        </aside>\n      </li>\n    </ul>\n  </li>\n  <li>next</li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

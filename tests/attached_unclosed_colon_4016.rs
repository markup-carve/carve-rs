#[test]
fn an_unclosed_admonition_releases_its_postblank_paragraph_and_lazy_line() {
    let source = "- a\n  - +\n  :::: note\n\n    p\ntail\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <aside class=\"admonition note\" aria-label=\"Note\">\n\n        </aside>\n        <p>p\ntail</p>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn an_unclosed_line_block_releases_its_postblank_stanza() {
    let source = "- a\n  - +\n  ::: |\n\n    p\ntail\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n        <p>p\ntail</p>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_sibling_after_the_unclosed_colon_block_stays_outside() {
    let source = "- a\n  - +\n  :::: note\n\n    p\ntail\n- next\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <aside class=\"admonition note\" aria-label=\"Note\">\n\n        </aside>\n        <p>p\ntail</p>\n      </li>\n    </ul>\n  </li>\n  <li>next</li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
#[test]
fn marker_line_plus_releases_an_unclosed_note_at_its_first_blank() {
    let source = "- +\n::: note\n\n  tail\n";
    let expected = "<ul>\n  <li>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n\n    </aside>\n    <p>tail</p>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_closed_note_still_owns_its_interior_blank() {
    let source = "- +\n::: note\n\n  tail\n:::\n";
    let expected = "<ul>\n  <li>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>tail</p>\n    </aside>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_colon_run_inside_a_later_code_fence_does_not_close_the_attached_note() {
    let source = "- +\n::: note\n\n  tail\n```\n:::\n```\n";
    let expected = "<ul>\n  <li>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n\n    </aside>\n    <p>tail</p>\n  </li>\n</ul>\n<pre><code>:::\n</code></pre>";
    assert_eq!(carve::to_html(source), expected);
}

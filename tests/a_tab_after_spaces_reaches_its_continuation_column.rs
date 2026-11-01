use carve::to_html;

#[test]
fn a_tab_after_spaces_reaches_the_child_markers_column() {
    assert_eq!(
        to_html("- x\n  - y\n    - a\n    +\n  \t> q\n").trim(),
        "<ul>\n  <li>x\n    <ul>\n      <li>y\n        <ul>\n          <li>a\n            <blockquote><p>q</p></blockquote>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_quote_body_keeps_its_existing_tab_phase() {
    assert_eq!(
        to_html("> - a\n>   - b\n>     - c\n>     +\n>   \t> q\n").trim(),
        "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n          <ul>\n            <li>c\n+\n              <blockquote><p>q</p></blockquote>\n            </li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>"
    );
}

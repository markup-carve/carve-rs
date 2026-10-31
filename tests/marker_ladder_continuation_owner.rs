use carve::to_html;

#[test]
fn an_accepted_plus_at_the_outer_marker_column_interrupts_the_term() {
    assert_eq!(
        to_html("- * :: t\n+\n: a\n").trim(),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n      </li>\n    </ul>\n    : a\n  </li>\n</ul>"
    );
}

#[test]
fn an_accepted_plus_at_the_child_marker_column_interrupts_the_term() {
    assert_eq!(
        to_html("- * :: t\n  +\n  : a\n").trim(),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n        : a\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_second_plus_after_a_quote_stays_with_the_child() {
    assert_eq!(
        to_html("- - :: t\n  +\n  > q\n  +\n  :: s\n").trim(),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n        <blockquote><p>q</p></blockquote>\n        <dl>\n          <dt>s</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_second_plus_after_a_description_stays_with_the_child() {
    assert_eq!(
        to_html("- - :: t\n    : d\n  +\n  :: s\n").trim(),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n          <dd>d</dd>\n          <dt>s</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_heading_at_an_intermediate_content_column_stays_with_that_item() {
    assert_eq!(
        to_html("- - - - :: t\n      # h\n").trim(),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <ul>\n              <li>\n                <dl>\n                  <dt>t</dt>\n                </dl>\n              </li>\n            </ul>\n            <h1 id=\"h\">h</h1>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn description_shaped_line_below_a_nested_item_stays_in_its_verse() {
    let source = "- a\n  - b\n    :: t\n    : ::: |\n      v\n      %%%\n : d\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <dl>\n          <dt>t</dt>\n          <dd>\n            <div class=\"line-block\">\n              <p>v<br>\n: d</p>\n            </div>\n          </dd>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_closed_span_keeps_the_nested_verse_open_for_a_low_description_line() {
    let source = "- a\n  - b\n    :: t\n    : ::: |\n      v\n      %%%\n        %%%\n : d\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <dl>\n          <dt>t</dt>\n          <dd>\n            <div class=\"line-block\">\n              <p>v<br>\n: d</p>\n            </div>\n          </dd>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn description_line_at_document_marker_does_not_enter_the_verse() {
    let head = "- a\n  - b\n    :: t\n    : ::: |\n      v\n      %%%\n";
    let at_document = carve::to_html(&(head.to_owned() + ": d\n"));
    assert!(at_document.ends_with("</ul>\n<p>: d</p>"), "{at_document}");
    assert!(at_document.contains("<p>v</p>"), "{at_document}");
}

#[test]
fn a_separator_below_an_item_term_closes_the_item() {
    let source = "- :: term\nflush block\n[l](u)\n: A definition wrapped\nonto the next line.\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>\n    <dl>\n      <dt>term\nflush block\n<a href=\"u\">l</a></dt>\n    </dl>\n  </li>\n</ul>\n<p>: A definition wrapped\nonto the next line.</p>"
    );
}

#[test]
fn a_separator_at_the_items_content_column_still_starts_its_body() {
    let source = "- :: term\n  : body\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>\n    <dl>\n      <dt>term</dt>\n      <dd>body</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn a_separator_reaching_only_the_outer_item_cannot_use_the_inner_term() {
    let source = "- - :: term\n  : outer text\n";
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <dl>\n          <dt>term</dt>\n        </dl>\n      </li>\n    </ul>\n    : outer text\n  </li>\n</ul>"
    );
}

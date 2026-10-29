use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

#[test]
fn separator_at_document_column_ends_the_item() {
    assert_eq!(
        html("- a\n  :: m\n: l\n: l\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>m</dt>\n    </dl>\n  </li>\n</ul>\n<p>: l\n: l</p>"
    );
}

#[test]
fn separator_one_column_in_folds_into_the_term() {
    assert_eq!(
        html("- a\n  :: m\n : l\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>m\n: l</dt>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn separator_at_the_list_column_opens_a_description() {
    assert_eq!(
        html("- a\n  :: m\n  : l\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>m</dt>\n      <dd>l</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn marker_line_term_uses_the_item_content_column() {
    assert_eq!(
        html("- :: t\n: - a\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n    </dl>\n  </li>\n</ul>\n<p>: - a</p>"
    );
}

#[test]
fn wider_item_uses_its_own_content_column() {
    assert_eq!(
        html("10. a\n    :: m\n   : l\n    : d\n"),
        "<ol start=\"10\">\n  <li>a\n    <dl>\n      <dt>m\n: l</dt>\n      <dd>d</dd>\n    </dl>\n  </li>\n</ol>"
    );
}

#[test]
fn separator_below_an_open_description_folds_into_it() {
    assert_eq!(
        html("- a\n  :: m\n  : d\n : l\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>m</dt>\n      <dd>d\n: l</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn quoted_lazy_separator_stays_with_the_term() {
    assert_eq!(
        html("> - a\n>   :: m\n>  : l\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <dl>\n        <dt>m\n: l</dt>\n      </dl>\n    </li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn closed_definition_list_does_not_claim_a_later_paragraph() {
    assert_eq!(
        html("- a\n  :: m\n  : d\n\n  para\n: l\n"),
        "<ul>\n  <li><p>a</p>\n    <dl>\n      <dt>m</dt>\n      <dd>d</dd>\n    </dl>\n    <p>para\n: l</p>\n  </li>\n</ul>"
    );
}

#[test]
fn term_text_inside_code_does_not_open_a_definition_list() {
    assert_eq!(
        html("- a\n  ```\n  :: m\n  ```\n  para\n: l\n"),
        "<ul>\n  <li>a\n    <pre><code>:: m\n</code></pre>\n    para\n: l\n  </li>\n</ul>"
    );
}

#[test]
fn deeper_term_text_does_not_move_the_list_column() {
    assert_eq!(
        html("- a\n  :: m\n   :: n\n  : d\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>m</dt>\n      <dt>n</dt>\n      <dd>d</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

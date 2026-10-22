use carve::{to_html, to_html_with_options, Options};

fn html(source: &str) -> String {
    let plain = to_html(source);
    assert_eq!(
        plain,
        to_html_with_options(source, &Options::default().with_positions(true)),
        "position path differs for {source:?}"
    );
    plain
}

#[test]
fn below_column_term_folds_into_the_open_term() {
    assert_eq!(
        html("- a\n  :: term\n :: next\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>term\n:: next</dt>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn quoted_marker_column_separator_follows_the_list() {
    assert_eq!(
        html("> - a\n>   :: term\n> : detail\n"),
        "<blockquote>\n  <ul>\n    <li>a\n      <dl>\n        <dt>term</dt>\n      </dl>\n    </li>\n  </ul>\n  <p>: detail</p>\n</blockquote>"
    );
}

#[test]
fn unquoted_marker_column_separator_follows_the_list() {
    assert_eq!(
        html("- a\n  :: term\n: detail\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>term</dt>\n    </dl>\n  </li>\n</ul>\n<p>: detail</p>"
    );
}

#[test]
fn outer_column_term_starts_a_new_definition_list() {
    assert_eq!(
        html("- a\n  :: term\n:: next\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>term</dt>\n    </dl>\n  </li>\n</ul>\n<dl>\n  <dt>next</dt>\n</dl>"
    );
}

#[test]
fn nested_item_marker_column_does_not_fold_into_its_term() {
    for source in [
        "- a\n  - b\n    :: term\n  :: next\n",
        "> - a\n>   - b\n>     :: term\n>   :: next\n",
    ] {
        let rendered = html(source);
        assert!(rendered.contains("<dt>term</dt>"), "{rendered}");
        assert!(rendered.contains("<dt>next</dt>"), "{rendered}");
        assert!(!rendered.contains("<dt>term\n:: next</dt>"), "{rendered}");
    }
}

#[test]
fn separator_at_a_nested_marker_column_ends_the_nested_item() {
    assert_eq!(
        html("- - a\n    :: m\n  : l\n  : l\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n        <dl>\n          <dt>m</dt>\n        </dl>\n      </li>\n    </ul>\n    : l\n: l\n  </li>\n</ul>"
    );
    for source in [
        "- x\n  - a\n    :: m\n  : l\n",
        "> - - a\n>     :: m\n>   : l\n",
    ] {
        let rendered = html(source);
        let flat = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("<dt>m</dt> </dl> </li> </ul> : l </li>"), "{rendered}");
    }
}

#[test]
fn flush_separator_leaves_a_definition_list_that_ends_a_nested_item() {
    assert_eq!(
        html("- x\n  - a\n    :: m\n: l\n"),
        "<ul>\n  <li>x\n    <ul>\n      <li>a\n        <dl>\n          <dt>m</dt>\n        </dl>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>: l</p>"
    );
}

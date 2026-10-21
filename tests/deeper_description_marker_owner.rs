fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_deeper_description_releases_a_list_to_the_outer_body() {
    let source = ":: T\n: D\n    :: t\n    : d\n     - b\n";
    let expected = "<dl>\n  <dt>T</dt>\n  <dd>\n    <p>D</p>\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>b</li>\n    </ul>\n  </dd>\n</dl>";
    assert_eq!(html(source), expected);
    let positioned = carve::Options {
        positions: true,
        ..Default::default()
    };
    assert_eq!(carve::to_html_with_options(source, &positioned), expected);
}

#[test]
fn an_item_host_keeps_the_list_in_its_outer_description() {
    let source = "- a0\n  :: T\n  : D\n    :: t\n    : d\n     - b\n";
    let expected = "<ul>\n  <li>a0\n    <dl>\n      <dt>T</dt>\n      <dd>\n        <p>D</p>\n        <dl>\n          <dt>t</dt>\n          <dd>d</dd>\n        </dl>\n        <ul>\n          <li>b</li>\n        </ul>\n      </dd>\n    </dl>\n  </li>\n</ul>";
    assert_eq!(html(source), expected);
}

#[test]
fn marker_kind_depth_and_quote_host_keep_the_outer_owner() {
    let cases = [
        (
            ":: T\n: D\n      :: t\n      : d\n       1. b\n",
            "    <ol>\n      <li>b</li>\n    </ol>",
        ),
        (
            ":: T\n: D\n    :: t\n    : d\n     * [ ] b\n",
            "    <ul class=\"task-list\">\n      <li><input type=\"checkbox\" disabled aria-label=\"b\"> b</li>\n    </ul>",
        ),
        (
            "> :: T\n> : D\n>     :: t\n>     : d\n>      - b\n",
            "      <ul>\n        <li>b</li>\n      </ul>",
        ),
    ];
    for (source, list) in cases {
        let rendered = html(source);
        assert!(
            rendered.contains(&format!("</dl>\n{list}")),
            "the outer description should own the list: {source:?}\n{rendered}"
        );
    }
}

#[test]
fn a_marker_below_both_body_columns_stays_literal() {
    let source = ":: T\n: D\n    :: t\n    : d\n - b\n";
    assert_eq!(
        html(source),
        "<dl>\n  <dt>T</dt>\n  <dd>\n    <p>D</p>\n    <dl>\n      <dt>t</dt>\n      <dd>d\n- b</dd>\n    </dl>\n  </dd>\n</dl>"
    );
}

#[test]
fn many_deeper_siblings_need_one_column_check_per_marker() {
    let source = ":: T\n: D\n    :: t\n    : d\n     - b\n".repeat(128);
    let rendered = html(&source);
    assert_eq!(rendered.matches("<li>b</li>").count(), 128);
}

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
fn corpus_620_separator_at_outer_marker_column_ends_the_item() {
    assert_eq!(
        html("- :: t\n:  head\n\n     a\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n    </dl>\n  </li>\n</ul>\n<p>:  head</p>\n<p>a</p>"
    );
}

#[test]
fn marker_line_term_uses_the_item_content_column_at_every_host() {
    assert_eq!(
        html("- :: t\n :  head\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t\n:  head</dt>\n    </dl>\n  </li>\n</ul>"
    );
    assert_eq!(
        html("- :: t\n  :  head\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>head</dd>\n    </dl>\n  </li>\n</ul>"
    );
    assert!(html("- intro\n  :: t\n:  head\n").ends_with("</ul>\n<p>:  head</p>"));
    assert!(html("1. :: t\n:  head\n").ends_with("</ol>\n<p>:  head</p>"));
    assert!(html("> - :: t\n> :  head\n").contains("</ul>\n  <p>:  head</p>\n</blockquote>"));
    assert!(html("> - :: t\n>  :  head\n").contains("<dt>t\n:  head</dt>"));
    assert!(html("> - :: t\n>   :  head\n").contains("<dd>head</dd>"));
}

#[test]
fn a_separator_inside_the_marker_column_stays_in_the_open_term_after_lazy_text() {
    assert_eq!(
        html("- :: term\nflush block\n : body\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>term\nflush block\n: body</dt>\n    </dl>\n  </li>\n</ul>"
    );
}

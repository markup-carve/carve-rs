fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_quoted_note_under_an_authored_item_base_releases_the_document_line() {
    let source = "- h\n    > q\n    > [^n]: n\n    >   m\nx\n";
    let expected = "<ul>\n  <li>h\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>\n<p>x</p>";
    assert_eq!(html(source), expected);
    let positioned = carve::Options {
        positions: true,
        ..Default::default()
    };
    assert_eq!(carve::to_html_with_options(source, &positioned), expected);
}

#[test]
fn the_same_owner_survives_another_list_depth() {
    let source = "- o\n  - h\n      > q\n      > [^n]: n\n      >   m\nx\n";
    assert_eq!(
        html(source),
        "<ul>\n  <li>o\n    <ul>\n      <li>h\n        <blockquote><p>q</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"
    );
}

#[test]
fn an_at_column_quote_and_visible_prose_keep_their_prior_readings() {
    assert_eq!(
        html("- h\n  > q\n  > [^n]: n\n  >   m\nx\n"),
        "<ul>\n  <li>h\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>\n<p>x</p>"
    );
    assert_eq!(
        html("- h\n    > q\n    > m\nx\n"),
        "<ul>\n  <li>h\n    <blockquote><p>q\nm\nx</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn repeated_quoted_notes_each_release_the_next_document_line() {
    let mut source = String::new();
    let mut expected = String::new();
    for index in 0..128 {
        source.push_str(&format!(
            "- h\n    > q\n    > [^n{index}]: n\n    >   m\nx{index}\n\n"
        ));
        expected.push_str(&format!(
            "<ul>\n  <li>h\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>\n<p>x{index}</p>\n"
        ));
    }
    assert_eq!(html(&source), expected.trim_end());
}

#[test]
fn an_outer_quote_keeps_the_document_line_outside_the_inner_item() {
    let source =
        "> - h0\n>   1. h1\n>       > quote\n>       > [^n]: note\n>       >   more\n> q\n";
    assert_eq!(
        html(source),
        "<blockquote>\n  <ul>\n    <li>h0\n      <ol>\n        <li>h1\n          <blockquote><p>quote</p></blockquote>\n        </li>\n      </ol>\n    </li>\n  </ul>\n  <p>q</p>\n</blockquote>"
    );
}

#[test]
fn an_outer_quote_link_definition_ends_the_inner_quote_paragraph() {
    let source = "> - h0\n>   1. h1\n>     :: term\n>     :  detail\n>      > quote\n>      > [n]: /u\n> q\n";
    assert_eq!(
        html(source),
        "<blockquote>\n  <ul>\n    <li>h0\n      <ol>\n        <li>h1</li>\n      </ol>\n      <dl>\n        <dt>term</dt>\n        <dd>detail</dd>\n      </dl>\n      <blockquote><p>quote</p></blockquote>\n    </li>\n  </ul>\n  <p>q</p>\n</blockquote>"
    );
}

#[test]
fn a_visible_reference_shaped_line_still_folds_into_the_quote() {
    let source = "> - h0\n>   1. h1\n>     :: term\n>     :  detail\n>      > quote\n>      > [n] : /u\n> q\n";
    assert_eq!(
        html(source),
        "<blockquote>\n  <ul>\n    <li>h0\n      <ol>\n        <li>h1</li>\n      </ol>\n      <dl>\n        <dt>term</dt>\n        <dd>detail</dd>\n      </dl>\n      <blockquote><p>quote\n[n] : /u\nq</p></blockquote>\n    </li>\n  </ul>\n</blockquote>"
    );
}

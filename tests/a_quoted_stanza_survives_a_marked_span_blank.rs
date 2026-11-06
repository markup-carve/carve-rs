//! PART 9 §23: a marked blank inside a verse span leaves the prior stanza open.

#[test]
fn a_quoted_description_stanza_takes_the_unmarked_follower() {
    let source = "> :: t\n> : ::: |\n>   x\n>   %%%\n>\nb\n";
    assert_eq!(
        carve::to_html(source).trim_end(),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <div class=\"line-block\">\n        <p>x<br>\nb</p>\n      </div>\n    </dd>\n  </dl>\n</blockquote>"
    );
}

#[test]
fn a_quoted_item_stanza_takes_the_unmarked_follower() {
    let source = "> - ::: |\n>   x\n>   %%%\n>\nb\n";
    let html = carve::to_html(source);
    assert!(html.contains("<p>x<br>\nb</p>"), "{html}");
    assert!(!html.contains("<p>b</p>"), "{html}");
}

#[test]
fn an_empty_stanza_still_releases_the_unmarked_follower() {
    let source = "> :: t\n> : ::: |\n>   %%%\n>\nb\n";
    let html = carve::to_html(source);
    assert!(
        html.trim_end().ends_with("</blockquote>\n<p>b</p>"),
        "{html}"
    );
}

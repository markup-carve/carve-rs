#[test]
fn repeated_empty_quoted_items_release_flush_left_tail() {
    assert_eq!(
        carve::to_html("> . >\n> . >\nX"),
        "<blockquote>\n  <ol>\n    <li>\n      <blockquote>\n\n      </blockquote>\n    </li>\n    <li>\n      <blockquote>\n\n      </blockquote>\n    </li>\n  </ol>\n</blockquote>\n<p>X</p>"
    );
}

#[test]
fn nested_closed_fence_releases_flush_left_tail() {
    assert_eq!(
        carve::to_html("> > - > ~~~\n  > x\n  > y\n  > ~~~"),
        "<blockquote>\n  <blockquote>\n    <ul>\n      <li>\n        <blockquote>\n          <pre><code></code></pre>\n        </blockquote>\n      </li>\n    </ul>\n  </blockquote>\n  <p>x\ny</p>\n  <pre><code></code></pre>\n</blockquote>"
    );
}

#[test]
fn empty_attributed_item_releases_flush_left_tail() {
    assert_eq!(
        carve::to_html("> > -{#k} {#h}\n # h"),
        "<blockquote>\n  <blockquote>\n    <ul>\n      <li id=\"k\"></li>\n    </ul>\n  </blockquote>\n</blockquote>\n<section id=\"h\">\n  <h1>h</h1>\n</section>"
    );
}

#[test]
fn nested_closed_div_releases_flush_left_tail() {
    assert_eq!(
        carve::to_html("> > - ::: box\ntext"),
        "<blockquote>\n  <blockquote>\n    <ul>\n      <li>\n        <div class=\"box\">\n\n        </div>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>text</p>"
    );
}

//! A comment does not revive a list item that holds no open paragraph
//! (markup-carve/carve-rs#2109).
//!
//! Section 24 C3 says a comment does not CLOSE the item, and it does not reach
//! it either - so once the item's own block has closed its paragraph, PART 1 S4
//! has already ended the item and the comment belongs to the document with
//! whatever follows it. The six `506-comment-columns-and-surviving-list-items`
//! documents are the corpus's own spelling of this; the rest are the same rule
//! over the other marker-line blocks that leave no paragraph.
//!
//! Every expectation here is agreed by the executable spec at markup-carve/carve
//! d20ebd942, carve-js 085d4edac and carve-php 1ce00b218, each run per shape.

#[test]
fn c0_comment_then_tail() {
    assert_eq!(
        carve::to_html("- intro\n%% a\ntail\n"),
        "<ul>\n  <li>intro\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn chunk_fence_c0comment_lazy() {
    assert_eq!(
        carve::to_html("- a\n\n  ```\n  code\n  ```\n%% c\nmore\n"),
        "<ul>\n  <li>a\n    <pre><code>code\n</code></pre>\n  </li>\n</ul>\n<p>more</p>"
    );
}

#[test]
fn d0() {
    assert_eq!(
        carve::to_html("- intro\n%% a\n%% b\ntail\n"),
        "<ul>\n  <li>intro\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn d5() {
    assert_eq!(
        carve::to_html("- intro\n%% a\n  %% b\ntail\n"),
        "<ul>\n  <li>intro</li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn d6() {
    assert_eq!(
        carve::to_html("- intro\n  %% a\n%% b\ntail\n"),
        "<ul>\n  <li>intro</li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn empty_item_c0comment() {
    assert_eq!(carve::to_html("-\n%% c\ntail\n"), "<p>-</p>\n<p>tail</p>");
}

#[test]
fn i_comment_only() {
    assert_eq!(
        carve::to_html("- intro\n  %% a\n"),
        "<ul>\n  <li>intro</li>\n</ul>"
    );
}

#[test]
fn i_comment_then_c0_tail() {
    assert_eq!(
        carve::to_html("- intro\n  %% a\n%% b\ntail\n"),
        "<ul>\n  <li>intro</li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn i_comment_then_indented() {
    assert_eq!(
        carve::to_html("- intro\n  %% a\n  tail\n"),
        "<ul>\n  <li>intro\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn i_comment_then_tail() {
    assert_eq!(
        carve::to_html("- intro\n  %% a\ntail\n"),
        "<ul>\n  <li>intro</li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn ichunk_heading_c0comment() {
    assert_eq!(
        carve::to_html("- a\n\n  # h\n%% c\ntail\n"),
        "<ul>\n  <li>a\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn icomment_c0comment_eof() {
    assert_eq!(
        carve::to_html("- intro\n  %% a\n%% b\n"),
        "<ul>\n  <li>intro</li>\n</ul>"
    );
}

#[test]
fn marker_fence_c0comment() {
    assert_eq!(
        carve::to_html("- ```\n  c\n  ```\n%% c\ntail\n"),
        "<ul>\n  <li>\n    <pre><code>c\n</code></pre>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn marker_heading_c0comment() {
    assert_eq!(
        carve::to_html("- # h\n%% c\ntail\n"),
        "<ul>\n  <li>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn marker_quote_c0comment() {
    assert_eq!(
        carve::to_html("- > q\n%% c\ntail\n"),
        "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn marker_table_c0comment() {
    assert_eq!(
        carve::to_html("- |a|\n%% c\ntail\n"),
        "<ul>\n  <li>\n    <table>\n      <tbody>\n        <tr><td>a</td></tr>\n      </tbody>\n    </table>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn marker_text_c0comment() {
    assert_eq!(
        carve::to_html("- a\n%% c\ntail\n"),
        "<ul>\n  <li>a\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn para_then_i_comment_tail() {
    assert_eq!(
        carve::to_html("- intro\n\n  body\n  %% a\ntail\n"),
        "<ul>\n  <li><p>intro</p>\n    <p>body</p>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn two_i_comments_tail() {
    assert_eq!(
        carve::to_html("- intro\n  %% a\n  %% b\ntail\n"),
        "<ul>\n  <li>intro</li>\n</ul>\n<p>tail</p>"
    );
}

// PART 9 §10 I2 folds a lazy line into the deepest open item paragraph.

#[test]
fn minimal() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\n   q\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn column_0() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\nq\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn column_2() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\n  q\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn column_4() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\n    q\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn no_description() {
    let src = "- a\n    - a\n      - b\n   q\n";
    let want = "<ul>\n  <li>a\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn task_items() {
    let src = "* [ ] task\n  :: term\n  :  detail\n    - a\n      * [ ] task\n   q\n";
    let want = "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled aria-label=\"task\"> task\n    <dl>\n      <dt>term</dt>\n      <dd>detail</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul class=\"task-list\">\n          <li><input type=\"checkbox\" disabled aria-label=\"task q\"> task\nq</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn quoted_host() {
    let src = "> - a\n>   :: t\n>   :  d\n>     - a\n>       - b\n>    q\n";
    let want = "<blockquote>\n  <ul>\n    <li>a\n      <dl>\n        <dt>t</dt>\n        <dd>d</dd>\n      </dl>\n      <ul>\n        <li>a\n          <ul>\n            <li>b\nq</li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn following_table() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\n   q\n\n   | a |\n   | b |\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq</li>\n        </ul>\n      </li>\n    </ul>\n    <table>\n      <tbody>\n        <tr><td>a</td></tr>\n        <tr><td>b</td></tr>\n      </tbody>\n    </table>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn following_ordered_list() {
    let src = "- a\n  :: term\n  :  detail\n    - a\n   q  \n   1. ordered\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>term</dt>\n      <dd>detail</dd>\n    </dl>\n    <ul>\n      <li>a\nq</li>\n    </ul>\n    <ol>\n      <li>ordered</li>\n    </ol>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn consecutive_lazy_lines() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\n   q\n   r\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq\nr</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn quoted_consecutive_lazy_lines() {
    let src = "> - a\n>   :: t\n>   :  d\n>     - a\n>       - b\n>    q\n>    r\n";
    let want = "<blockquote>\n  <ul>\n    <li>a\n      <dl>\n        <dt>t</dt>\n        <dd>d</dd>\n      </dl>\n      <ul>\n        <li>a\n          <ul>\n            <li>b\nq\nr</li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn definition_after_nested_marker() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\n   [x]: /u\n   q\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b</li>\n        </ul>\n      </li>\n    </ul>\n    q\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn mixed_column_lazy_lines() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\nq\n   r\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq\nr</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn quoted_mixed_column_lazy_lines() {
    let src = "> - a\n>   :: t\n>   :  d\n>     - a\n>       - b\n> q\n>    r\n";
    let want = "<blockquote>\n  <ul>\n    <li>a\n      <dl>\n        <dt>t</dt>\n        <dd>d</dd>\n      </dl>\n      <ul>\n        <li>a\n          <ul>\n            <li>b\nq\nr</li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn indented_line_between_lazy_lines() {
    let src = "- a\n  :: t\n  :  d\n    - a\n      - b\n   q\n        r\n   s\n";
    let want = "<ul>\n  <li>a\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\nq\nr\ns</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(src), want);
}

#[test]
fn quoted_indented_line_between_lazy_lines() {
    let src = "> - a\n>   :: t\n>   :  d\n>     - a\n>       - b\n>    q\n>         r\n>    s\n";
    let want = "<blockquote>\n  <ul>\n    <li>a\n      <dl>\n        <dt>t</dt>\n        <dd>d</dd>\n      </dl>\n      <ul>\n        <li>a\n          <ul>\n            <li>b\nq\nr\ns</li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(carve::to_html(src), want);
}

use carve::to_html;

// Rule A walk (carve#3885) for a bullet task item whose marker line opens a
// child: the item reader measures that child past the task box.
fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source).trim(), expected, "{source:?}");
}

#[test]
fn a_sibling_term_one_column_past_the_child_marker_opens_in_the_task_item() {
    check(
        "* [ ] - :: t\n       : a\n       :: sibling\n       tail\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: a</dt>\n        </dl>\n      </li>\n    </ul>\n    <dl>\n      <dt>sibling\ntail</dt>\n    </dl>\n  </li>\n</ul>",
    );
}

#[test]
fn a_sibling_term_one_column_below_the_child_marker_opens_in_the_task_item() {
    check(
        "* [ ] - :: t\n     : a\n     :: sibling\n     tail\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>\n        <dl>\n          <dt>t\n: a</dt>\n        </dl>\n      </li>\n    </ul>\n    <dl>\n      <dt>sibling\ntail</dt>\n    </dl>\n  </li>\n</ul>",
    );
}

#[test]
fn a_heading_at_the_task_items_column_follows_the_child_list() {
    check(
        "- [ ] - a\n  # b\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>a</li>\n    </ul>\n    <h1 id=\"b\">b</h1>\n  </li>\n</ul>",
    );
}

#[test]
fn a_heading_at_the_child_marker_column_follows_the_child_list() {
    check(
        "- [ ] - a\n      # b\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>a</li>\n    </ul>\n    <h1 id=\"b\">b</h1>\n  </li>\n</ul>",
    );
}

#[test]
fn a_marker_at_the_child_content_column_nests_in_the_child() {
    check(
        "- [ ] - a\n        - b\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>a\n        <ul>\n          <li>b</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_marker_one_column_past_the_child_marker_is_its_sibling() {
    check(
        "- [ ] - a\n       - b\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>a</li>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_marker_of_another_kind_at_the_items_column_opens_a_list_in_the_item() {
    check(
        "- [ ] - a\n  1. b\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>a</li>\n    </ul>\n    <ol>\n      <li>b</li>\n    </ol>\n  </li>\n</ul>",
    );
}

#[test]
fn a_marker_of_another_kind_below_the_child_marker_is_lazy() {
    check(
        "- [ ] 1. a\n     - b\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ol>\n      <li>a\n- b</li>\n    </ol>\n  </li>\n</ul>",
    );
}

#[test]
fn a_fence_in_the_task_item_keeps_its_payload_indentation() {
    check(
        "- [x] - a\n  ```\n    x\n  ```\n",
        "<ul class=\"task-list\">\n  <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled> \n    <ul>\n      <li>a</li>\n    </ul>\n    <pre><code>  x\n</code></pre>\n  </li>\n</ul>",
    );
}

#[test]
fn a_lazy_line_stays_in_the_child_paragraph() {
    check(
        "- [ ] - a\n   lazy\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>a\nlazy</li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn a_task_item_in_a_quote_reads_its_child_the_same_way() {
    check(
        "> - [ ] - a\n>   # b\n",
        "<blockquote>\n  <ul class=\"task-list\">\n    <li><input type=\"checkbox\" disabled> \n      <ul>\n        <li>a</li>\n      </ul>\n      <h1 id=\"b\">b</h1>\n    </li>\n  </ul>\n</blockquote>",
    );
}
#[test]
fn a_plus_at_the_child_marker_column_attaches_after_the_term() {
    check(
        "- [ ] - :: t\n      +\n      x\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <ul>\n      <li>\n        <dl>\n          <dt>t</dt>\n        </dl>\n        x\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

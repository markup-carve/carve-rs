use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_task_marker_line_run_uses_the_brace_source_column() {
    check(
        "- [ ] {.x\n      .y}\nz\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled aria-label=\"{.x .y} z\"> {.x\n.y}\nz</li>\n</ul>",
    );
    check(
        "- [ ] {.x\n       .y}\nz\n",
        "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> </li>\n</ul>\n<p>z</p>",
    );
}

#[test]
fn a_repeated_marker_attribute_does_not_move_the_brace_column() {
    check(
        "-{.x} {.x\n     .y}\n  # h\n",
        "<ul>\n  <li class=\"x\">{.x\n.y}\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>",
    );
    check(
        "-{.a} {.x\n     .y}\n  # h\n",
        "<ul>\n  <li class=\"a\">{.x\n.y}\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>",
    );
    check(
        "-{.x} {.x\n       .y}\n  # h\n",
        "<ul>\n  <li class=\"x\">\n    <h1 class=\"x y\" id=\"h\">h</h1>\n  </li>\n</ul>",
    );
}

//! PART 10 §1: a generated base class takes the authored class slot's spelling.

use carve::to_html;

#[test]
fn a_task_list_keeps_the_authored_class_key_and_its_position() {
    assert_eq!(
        to_html("{CLASS=c}\n- [ ] a\n").trim_end(),
        "<ul CLASS=\"task-list c\">\n  <li><input type=\"checkbox\" disabled aria-label=\"a\"> a</li>\n</ul>"
    );
    assert_eq!(
        to_html("{id=z CLASS=c}\n- [ ] a\n").trim_end(),
        "<ul id=\"z\" CLASS=\"task-list c\">\n  <li><input type=\"checkbox\" disabled aria-label=\"a\"> a</li>\n</ul>"
    );
    assert_eq!(
        to_html("{CLASS=c .d}\n- [ ] a\n").trim_end(),
        "<ul CLASS=\"task-list c d\">\n  <li><input type=\"checkbox\" disabled aria-label=\"a\"> a</li>\n</ul>"
    );
    assert_eq!(
        to_html("{.d CLASS=c}\n- [ ] a\n").trim_end(),
        "<ul class=\"task-list d c\">\n  <li><input type=\"checkbox\" disabled aria-label=\"a\"> a</li>\n</ul>"
    );
}

#[test]
fn a_math_span_keeps_the_authored_class_key() {
    assert_eq!(
        to_html("$`a`{CLASS=c}\n").trim_end(),
        "<p><span CLASS=\"math inline c\" role=\"math\">\\(a\\)</span></p>"
    );
}

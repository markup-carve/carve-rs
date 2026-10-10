//! carve#2887: the list element of a task list carries PART 19 §1's mandatory
//! `task-list` base class, and the HTML importer reads it back as structure.

use carve::{html_to_carve, parse, render_carve, to_html, HtmlImportOptions};

fn fmt(source: &str) -> String {
    render_carve(&parse(source)).expect("writes")
}

fn import(html: &str) -> String {
    let imported = html_to_carve(html, &HtmlImportOptions::default()).expect("imports");
    fmt(&imported.value)
}

#[test]
fn a_task_list_carries_the_class() {
    assert_eq!(
        to_html("- [ ] open\n- [x] done\n- [X] shout\n- [-] dropped\n"),
        concat!(
            "<ul class=\"task-list\">\n",
            "  <li><input type=\"checkbox\" disabled aria-label=\"open\"> open</li>\n",
            "  <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled aria-label=\"done\"> done</li>\n",
            "  <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled aria-label=\"shout\"> shout</li>\n",
            "  <li data-task-state=\"-\"><input type=\"checkbox\" disabled aria-label=\"dropped\"> dropped</li>\n",
            "</ul>"
        )
    );
}

#[test]
fn the_class_leads_an_authored_class_in_its_slot() {
    assert!(to_html("{.c}\n- [ ] a\n").starts_with("<ul class=\"task-list c\">"));
    assert!(to_html("{#i .c}\n- [ ] a\n").starts_with("<ul id=\"i\" class=\"task-list c\">"));
    assert!(to_html("{.c #i}\n- [ ] a\n").starts_with("<ul class=\"task-list c\" id=\"i\">"));
    assert!(to_html("{#i}\n- [ ] a\n").starts_with("<ul class=\"task-list\" id=\"i\">"));
    assert!(to_html("{.task-list}\n- [ ] a\n").starts_with("<ul class=\"task-list\">"));
}

#[test]
fn a_plain_list_stays_bare_even_inside_a_task_item() {
    assert_eq!(
        to_html("- [x] parent\n  - child\n"),
        concat!(
            "<ul class=\"task-list\">\n",
            "  <li data-task-state=\"x\"><input type=\"checkbox\" checked disabled aria-label=\"parent\"> parent\n",
            "    <ul>\n",
            "      <li>child</li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>"
        )
    );
    assert!(to_html("- a\n").starts_with("<ul>"));
}

#[test]
fn the_importer_reads_the_class_as_structure() {
    assert_eq!(
        import("<ul class=\"task-list\"><li><input type=\"checkbox\"> a</li></ul>"),
        "- [ ] a\n"
    );
    assert_eq!(
        import("<ul class=\"task-list c\" id=\"i\"><li><input type=\"checkbox\"> a</li></ul>"),
        "{#i .c}\n- [ ] a\n"
    );
}

#[test]
fn the_class_on_a_list_that_is_not_a_task_list_is_the_authors() {
    assert_eq!(
        import("<ul class=\"task-list\"><li>a</li></ul>"),
        "{.task-list}\n- a\n"
    );
}

#[test]
fn a_rendered_task_list_survives_a_round_trip() {
    let source = fmt("{.c}\n- [ ] open\n- [x] done\n- [-] dropped\n  - child\n");
    assert_eq!(import(&to_html(&source)), source);
}

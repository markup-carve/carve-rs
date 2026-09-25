//! An ordered HTML task item keeps its bracket text and says what it lost
//! (carve-rs#1890).
//!
//! `resources/spec/03-blocks-core.ebnf` hangs `task_marker` off `unordered_item`
//! alone, so no Carve source carries a checkbox on an ordered item and no engine
//! can write the faithful answer. What the importer can do is keep the
//! characters the box was read from and stop claiming the conversion was clean:
//! `1. done` with an empty report told a reader nothing was lost.
//!
//! The row is `structure-unspellable`, which is the code for a structure the AST
//! holds and Carve source cannot spell (PART 12 §16). That is why `html_to_ast`
//! keeps the box and stays silent while `html_to_carve` flattens it and reports.
//!
//! Every case asserts the rendered HTML beside the Carve, because the bracket
//! text and the box look alike in neither.

use carve::{html_to_ast, html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportOptions};

struct Imported {
    carve: String,
    html: String,
    rows: Vec<(HtmlImportDiagnosticCode, String, String)>,
}

fn imported(html: &str) -> Imported {
    let result = html_to_carve(html, &HtmlImportOptions::default()).expect("import");
    Imported {
        html: to_html(&result.value),
        carve: result.value,
        rows: result
            .report
            .diagnostics
            .iter()
            .map(|d| {
                (
                    d.code,
                    d.path.clone().unwrap_or_default(),
                    d.message.clone(),
                )
            })
            .collect(),
    }
}

fn unspellable(rows: &[(HtmlImportDiagnosticCode, String, String)]) -> Vec<&str> {
    rows.iter()
        .filter(|(code, ..)| *code == HtmlImportDiagnosticCode::StructureUnspellable)
        .map(|(_, path, _)| path.as_str())
        .collect()
}

const BOX: &str = r#"<input type="checkbox""#;

#[test]
fn the_ticket_case_keeps_the_bracket_text_and_reports_the_loss() {
    let out = imported("<ol><li><input type=\"checkbox\" checked disabled> done</li></ol>");
    assert_eq!(out.carve, "1. [x] done\n");
    assert!(out.html.contains("<li>[x] done</li>"), "{}", out.html);
    assert_eq!(unspellable(&out.rows), ["/ol[1]/li[1]/input[1]"]);
    assert_eq!(
        out.rows[0].2,
        "Wrote an ordered task item's checkbox as its bracket text: a Carve task marker is spelled behind a bullet only, so the item keeps the characters and loses the task-item semantics"
    );
}

#[test]
fn an_unchecked_box_keeps_its_own_pair() {
    let out = imported("<ol><li><input type=\"checkbox\" disabled> open</li></ol>");
    assert_eq!(out.carve, "1. [ ] open\n");
    assert!(out.html.contains("<li>[ ] open</li>"), "{}", out.html);
    assert_eq!(unspellable(&out.rows).len(), 1);
}

#[test]
fn a_three_state_marker_keeps_the_character_it_was_written_with() {
    // `data-task-state` is the only carrier the HTML has (PART 10 §11), and the
    // bracket text is the marker the writer would have spelled behind a bullet -
    // so the state survives as text rather than collapsing to an empty box.
    let out = imported("<ol><li data-task-state=\"?\"><input type=\"checkbox\"> maybe</li></ol>");
    assert_eq!(out.carve, "1. [?] maybe\n");
    assert!(out.html.contains("<li>[?] maybe</li>"), "{}", out.html);
    assert_eq!(unspellable(&out.rows).len(), 1);
}

#[test]
fn a_box_that_is_the_whole_item_leaves_the_item_non_empty() {
    // The shape where dropping the box left an EMPTY item rather than a shorter
    // one, which is how carve-rs#1886 read on the Markdown side.
    let out = imported("<ol><li><input type=\"checkbox\" checked></li></ol>");
    assert_eq!(out.carve, "1. [x]\n");
    assert!(out.html.contains("<li>[x]</li>"), "{}", out.html);
    assert_eq!(unspellable(&out.rows).len(), 1);
}

#[test]
fn the_text_stands_where_the_input_stood() {
    // Not prepended to the item. A box in the middle of a run is unusual HTML
    // but it is what the document said, and moving it would be a second loss
    // nothing reports.
    let out = imported("<ol><li>before <input type=\"checkbox\" checked> after</li></ol>");
    assert_eq!(out.carve, "1. before [x] after\n");
    assert!(
        out.html.contains("<li>before [x] after</li>"),
        "{}",
        out.html
    );
}

#[test]
fn a_bullet_task_item_keeps_its_box_and_reports_nothing() {
    // THE CONTROL. It holds on both sides of the fix, so a change that simply
    // stopped reading HTML checkboxes fails here.
    let out = imported("<ul><li><input type=\"checkbox\" checked disabled> done</li></ul>");
    assert_eq!(out.carve, "- [x] done\n");
    assert!(out.html.contains(BOX), "{}", out.html);
    assert!(out.rows.is_empty(), "{:?}", out.rows);
}

#[test]
fn a_bullet_task_list_nested_in_an_ordered_item_keeps_its_boxes() {
    // The rule is the list the ITEM belongs to. A fix reading the outer list
    // would take the inner boxes away and report them as unspellable.
    let out = imported(
        "<ol><li><input type=\"checkbox\" checked><ul><li><input type=\"checkbox\"> in</li></ul></li></ol>",
    );
    assert_eq!(out.carve, "1. [x]\n   - [ ] in\n");
    assert!(out.html.contains(BOX), "{}", out.html);
    assert_eq!(unspellable(&out.rows), ["/ol[1]/li[1]/input[1]"]);
}

#[test]
fn an_ordered_task_list_nested_in_a_bullet_item_is_still_reported() {
    let out = imported(
        "<ul><li>outer<ol><li><input type=\"checkbox\" checked> inner</li></ol></li></ul>",
    );
    assert_eq!(out.carve, "- outer\n  1. [x] inner\n");
    assert_eq!(
        unspellable(&out.rows),
        ["/ul[1]/li[1]/ol[2]/li[1]/input[1]"]
    );
}

#[test]
fn the_ast_exit_keeps_the_box_and_says_nothing() {
    // PART 12 §16: only a WRITER loses this, so the published tree still carries
    // the state and the report has no row to carry.
    let result = html_to_ast(
        "<ol><li><input type=\"checkbox\" checked disabled> done</li></ol>",
        &HtmlImportOptions::default(),
    )
    .expect("import");
    assert!(result.report.diagnostics.is_empty(), "{:?}", result.report);
    let carve::BlockNode::List(list) = &result.value.children[0] else {
        panic!("expected a list, got {:?}", result.value.children[0]);
    };
    assert!(list.ordered);
    assert_eq!(list.items[0].checked, Some(true));
    // And the writer is where it goes, which is the loss the other exit reports.
    assert_eq!(carve::render_carve(&result.value).unwrap(), "1. done\n");
}

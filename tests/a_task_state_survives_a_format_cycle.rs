//! PART 11 §6g / carve#1866. The seven task states render as two, so the tree
//! carried only `checked` and the writer rewrote four of them to `[ ]`. The
//! state is the author's spelling, recorded like `List::bullet_char`.

use carve::{from_json, parse, render_carve, to_html, to_json};

fn fmt(source: &str) -> String {
    render_carve(&parse(source)).expect("writes")
}

#[test]
fn the_writer_puts_every_extended_state_back() {
    for state in ['-', '_', '>', '?'] {
        let source = format!("- [{state}] a\n");
        assert_eq!(fmt(&source), source);
    }
}

#[test]
fn the_state_is_recorded_only_when_it_is_not_the_default_for_the_box() {
    let items = |source: &str| parse(source).children[0].clone();
    match items("- [ ] a") {
        carve::BlockNode::List(list) => assert_eq!(list.items[0].task_state, None),
        other => panic!("not a list: {other:?}"),
    }
    match items("- [x] a") {
        carve::BlockNode::List(list) => assert_eq!(list.items[0].task_state, None),
        other => panic!("not a list: {other:?}"),
    }
    match items("- [-] a") {
        carve::BlockNode::List(list) => {
            assert_eq!(list.items[0].task_state, Some('-'));
            assert_eq!(list.items[0].checked, Some(false));
        }
        other => panic!("not a list: {other:?}"),
    }
}

#[test]
fn the_case_is_folded_because_it_is_not_a_state() {
    assert_eq!(fmt("- [X] a\n"), "- [x] a\n");
    assert_eq!(to_json(&parse("- [X] a")), to_json(&parse("- [x] a")));
}

#[test]
fn the_state_rides_the_wire() {
    let document = from_json(&to_json(&parse("- [>] deferred\n"))).expect("decodes");
    assert_eq!(render_carve(&document).expect("writes"), "- [>] deferred\n");
}

#[test]
fn an_item_with_attributes_keeps_its_state() {
    assert_eq!(fmt("-{.c} [?] a\n"), "-{.c} [?] a\n");
}

#[test]
fn the_rendering_does_not_move() {
    let html = to_html("- [>] a\n");
    assert!(html.contains("<input type=\"checkbox\" disabled"), "{html}");
    assert!(!html.contains("checked"), "{html}");
}

#[test]
fn a_payload_whose_fields_disagree_is_refused() {
    let payload = r#"{"type":"document","srcByteLength":0,"children":[{"type":"list","ordered":false,"tight":true,"items":[{"type":"list_item","children":[],"checked":true,"taskState":"-"}]}]}"#;
    let error = from_json(payload).expect_err("a contradicting pair is refused");
    assert!(format!("{error}").contains("taskState"), "{error}");
}

#[test]
fn a_state_outside_the_enum_is_refused() {
    let payload = r#"{"type":"document","srcByteLength":0,"children":[{"type":"list","ordered":false,"tight":true,"items":[{"type":"list_item","children":[],"checked":false,"taskState":"!"}]}]}"#;
    let error = from_json(payload).expect_err("a state outside the enum is refused");
    assert!(format!("{error}").contains("taskState"), "{error}");
}

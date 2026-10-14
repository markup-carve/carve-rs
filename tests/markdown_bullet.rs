//! A change of bullet marker is what SEPARATES two adjacent lists in
//! CommonMark, so normalizing every bullet to `-` merges lists the source kept
//! apart. That is a meaning change, and what a renderer must not do (carve#352).

#[test]
fn keeps_two_adjacent_lists_apart() {
    let out = carve::to_markdown("- a\n- b\n\n* c\n* d\n");
    assert!(out.contains("- a"), "got: {out}");
    assert!(out.contains("* c"), "got: {out}");
}

#[test]
fn keeps_the_bullet_on_a_task_list() {
    assert!(carve::to_markdown("* [x] done\n").contains("* [x] done"));
}

#[test]
fn leaves_a_hyphen_list_alone() {
    assert!(carve::to_markdown("- a\n").contains("- a"));
}

#[test]
fn markdown_import_keeps_authored_bullet() {
    assert_eq!(carve::markdown_to_carve("* one\n* two\n"), "* one\n* two\n");
    assert_eq!(carve::markdown_to_carve("- one\n- two\n"), "- one\n- two\n");
}

#[test]
fn markdown_import_writes_a_plus_bullet_as_carve_js_does() {
    assert_eq!(carve::markdown_to_carve("+ one\n+ two\n"), "- one\n- two\n");
    assert_eq!(carve::markdown_to_carve("* x\n+ y\n"), "* x\n\n- y\n");
    assert_eq!(carve::markdown_to_carve("+ x\n- y\n"), "- x\n\n* y\n");
    assert_eq!(carve::markdown_to_carve("- x\n+ y\n"), "- x\n\n* y\n");
}

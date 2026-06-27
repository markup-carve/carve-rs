use carve::{stamp_carve, to_carve, to_html, StampForm};

const GENERATED_BY: &str = "carve-rs 0.1.0";
const LINE_MARKER: &str = "%% carve-version: 0.1; generated-by: carve-rs 0.1.0";
const BLOCK_MARKER: &str = "%%%\ncarve-version: 0.1\ngenerated-by: carve-rs 0.1.0\n%%%";

#[test]
fn one_liner_default_form() {
    assert_eq!(
        stamp_carve("a\n", GENERATED_BY, StampForm::Line),
        format!("a\n\n{LINE_MARKER}\n")
    );
}

#[test]
fn block_form() {
    assert_eq!(
        stamp_carve("a\n", GENERATED_BY, StampForm::Block),
        format!("a\n\n{BLOCK_MARKER}\n")
    );
}

#[test]
fn idempotent() {
    let stamped = stamp_carve("a\n", GENERATED_BY, StampForm::Line);
    assert_eq!(
        stamp_carve(&stamped, GENERATED_BY, StampForm::Line),
        stamped
    );
}

#[test]
fn restamp_replaces_other_form() {
    let line = stamp_carve("a\n", GENERATED_BY, StampForm::Line);
    let block = stamp_carve("a\n", GENERATED_BY, StampForm::Block);

    assert_eq!(stamp_carve(&line, GENERATED_BY, StampForm::Block), block);
    assert_eq!(stamp_carve(&block, GENERATED_BY, StampForm::Line), line);
}

#[test]
fn renders_nothing() {
    let unstamped = "a\n";
    let stamped = stamp_carve(unstamped, GENERATED_BY, StampForm::Line);
    assert_eq!(to_html(&stamped), to_html(unstamped));
}

#[test]
fn keeps_unrelated_trailing_comment() {
    assert_eq!(
        stamp_carve("a\n\n%% note\n", GENERATED_BY, StampForm::Line),
        format!("a\n\n%% note\n\n{LINE_MARKER}\n")
    );
}

#[test]
fn empty_doc_gets_bare_marker() {
    assert_eq!(
        stamp_carve("", GENERATED_BY, StampForm::Line),
        format!("{LINE_MARKER}\n")
    );
}

#[test]
fn plain_to_carve_preserves_existing_marker_byte_for_byte() {
    let source = format!("a\n\n{LINE_MARKER}\n");
    assert_eq!(to_carve(&source), source);
}

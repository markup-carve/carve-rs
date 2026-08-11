use carve::{
    needs_review, read_stamp, stamp_carve, to_carve, to_html, Stamp, StampForm, SPEC_VERSION,
};

const GENERATED_BY: &str = "carve-rs 0.1.0";
const LINE_MARKER: &str = "%% carve-version: 0.2; generated-by: carve-rs 0.1.0";
const BLOCK_MARKER: &str = "%%%\ncarve-version: 0.2\ngenerated-by: carve-rs 0.1.0\n%%%";

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

#[test]
fn read_stamp_returns_none_for_an_unstamped_document() {
    assert_eq!(read_stamp("# Title\n\ntext\n"), None);
    assert_eq!(read_stamp(""), None);
}

#[test]
fn read_stamp_recognizes_the_line_form() {
    assert_eq!(
        read_stamp(&format!("text\n\n{LINE_MARKER}\n")),
        Some(Stamp {
            version: "0.2".to_string(),
            generated_by: Some(GENERATED_BY.to_string()),
        })
    );
}

#[test]
fn read_stamp_recognizes_the_block_form() {
    let source = "text\n\n%%%\ncarve-version: 0.0.9\ngenerated-by: carve-js 0.0.9\n%%%\n";

    assert_eq!(
        read_stamp(source),
        Some(Stamp {
            version: "0.0.9".to_string(),
            generated_by: Some("carve-js 0.0.9".to_string()),
        })
    );
}

#[test]
fn read_stamp_ignores_an_unrelated_trailing_comment() {
    assert_eq!(read_stamp("text\n\n%% just a note\n"), None);
    assert_eq!(read_stamp("text\n\n%%%\njust a note\n%%%\n"), None);
}

#[test]
fn read_stamp_tolerates_a_missing_generated_by() {
    assert_eq!(
        read_stamp("text\n\n%% carve-version: 0.1\n"),
        Some(Stamp {
            version: "0.1".to_string(),
            generated_by: None,
        })
    );
}

#[test]
fn what_stamp_carve_writes_is_what_read_stamp_returns() {
    // The pair has to agree, or the upgrade procedure reads the wrong version.
    for form in [StampForm::Line, StampForm::Block] {
        let stamped = stamp_carve("text\n", GENERATED_BY, form);
        let stamp = read_stamp(&stamped).expect("stamped output must be readable");

        assert_eq!(stamp.version, SPEC_VERSION);
        assert_eq!(stamp.generated_by.as_deref(), Some(GENERATED_BY));
    }
}

#[test]
fn needs_review_compares_against_the_targeted_spec_version() {
    let current = format!("text\n\n%% carve-version: {SPEC_VERSION}; generated-by: x\n");
    assert!(!needs_review(&current, SPEC_VERSION));

    assert!(needs_review(
        "text\n\n%% carve-version: 0.0.9; generated-by: x\n",
        SPEC_VERSION
    ));

    // Unknown provenance answers true: assuming a document is current is unsafe.
    assert!(needs_review("text\n", SPEC_VERSION));

    // A document from a future version is not this engine's problem.
    assert!(!needs_review(
        "text\n\n%% carve-version: 99.0; generated-by: x\n",
        SPEC_VERSION
    ));
}

#[test]
fn version_segments_compare_numerically_and_pad() {
    // Spec versions carry two segments ("0.1") and engine versions three
    // ("0.1.0"). Comparing by segment count, or lexically, reports every stamped
    // document as stale.
    assert!(!needs_review(
        "a\n\n%% carve-version: 0.1; generated-by: x\n",
        "0.1.0"
    ));
    assert!(!needs_review(
        "a\n\n%% carve-version: 0.1.0; generated-by: x\n",
        "0.1"
    ));

    // "0.10" sorts before "0.9" as a string, but 10 > 9.
    assert!(!needs_review(
        "a\n\n%% carve-version: 0.10; generated-by: x\n",
        "0.9"
    ));
    assert!(needs_review(
        "a\n\n%% carve-version: 0.9; generated-by: x\n",
        "0.10"
    ));
}

// The point of a provenance marker is that ANOTHER engine can read it. These are
// the literal bytes carve-php and carve-js write, so a divergence in any writer
// fails here rather than in the field.
#[test]
fn reads_markers_written_by_the_sibling_engines() {
    let php_line = "# Hi\n\n%% carve-version: 0.1; generated-by: carve-php 0.1.0\n";
    assert_eq!(
        read_stamp(php_line).and_then(|s| s.generated_by),
        Some("carve-php 0.1.0".to_string())
    );

    let php_block = "# Hi\n\n%%%\ncarve-version: 0.1\ngenerated-by: carve-php 0.1.0\n%%%\n";
    assert_eq!(
        read_stamp(php_block).and_then(|s| s.generated_by),
        Some("carve-php 0.1.0".to_string())
    );

    let js_line = "# Hi\n\n%% carve-version: 0.1; generated-by: carve-js 0.1.0\n";
    assert_eq!(
        read_stamp(js_line).and_then(|s| s.generated_by),
        Some("carve-js 0.1.0".to_string())
    );
}

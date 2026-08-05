//! An ingested tree does not keep a footnote number it cannot justify.
//!
//! PART 12 §5 serializes footnote numbering so a consumer need not reimplement
//! PART 9R. On a parsed document the number always describes the document it
//! came from; on an INGESTED one it need not.
//!
//! Delete a footnote definition from a published tree - what an editor does when
//! a user removes one - and read it back. The reference no longer resolves, so
//! every engine renders the literal `[^a]`, but the number copied off the payload
//! still claimed a footnote that is not in the document (carve#758). carve-php
//! already dropped it; carve-js needed a different fix for the same defect.
//!
//! THE SAME PASS `parse` RUNS. It assigns as well as clears, which is right here
//! and would be wrong in carve-js: this engine numbers footnotes during `parse`,
//! so an ingested tree numbered the same way agrees with a parsed one and §6's
//! round trip holds. carve-js numbers during resolution instead, so there the
//! pass has to clear without assigning - two engines, one rule, two shapes.

/// SOURCE, and the same tree with its definition removed.
const SOURCE: &str = "see[^a]\n\n[^a]: note\n";

fn published(src: &str) -> String {
    carve::ast_json::to_json(&carve::parse(src))
}

/// The published tree with the `footnote` child deleted, as text.
fn without_definition() -> String {
    let json = published(SOURCE);
    // The definition is a document child; drop it by cutting the object out.
    let start = json
        .find("{\"type\":\"footnote\"")
        .expect("the fixture has a footnote definition to remove");
    let mut depth = 0usize;
    let bytes = json.as_bytes();
    let mut end = start;
    for (i, b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut out = String::with_capacity(json.len());
    out.push_str(&json[..start]);
    out.push_str(&json[end..]);
    // Remove the comma the deleted element left behind.
    let cleaned = out.replace(",]", "]").replace("[,", "[").replace(",,", ",");
    assert!(
        !cleaned.contains("\"type\":\"footnote\""),
        "the fixture failed to remove the definition: {cleaned}"
    );

    cleaned
}

/// Run the `carve` binary with `args`, feeding `input` on stdin, return stdout.
fn run_cli(args: &[&str], input: &str) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait carve binary");
    assert!(
        out.status.success(),
        "carve exited non-zero: {:?}",
        out.status
    );

    String::from_utf8(out.stdout).expect("utf8 stdout")
}

#[test]
fn a_parsed_document_publishes_the_number() {
    // The baseline. Every assertion below would also pass if the number had
    // stopped being published at all.
    assert!(published(SOURCE).contains("\"number\":1"));
}

#[test]
fn an_ingested_tree_without_the_definition_drops_it() {
    let doc = carve::ast_json::from_json(&without_definition()).expect("decodes");
    let out = carve::ast_json::to_json(&doc);

    assert!(
        !out.contains("\"number\""),
        "a number survived a deleted definition: {out}"
    );
}

#[test]
fn the_reference_itself_survives() {
    // The boundary: dropping the node would also satisfy the assertion above.
    let doc = carve::ast_json::from_json(&without_definition()).expect("decodes");
    let out = carve::ast_json::to_json(&doc);

    assert!(out.contains("\"footnote_ref\""), "{out}");
    assert!(out.contains("\"id\":\"a\""), "{out}");
}

#[test]
fn an_unedited_tree_round_trips_unchanged() {
    // §6. The pass runs on every ingest, so it must be a no-op on a tree nobody
    // edited - including keeping the number it arrived with.
    let json = published(SOURCE);
    let back = carve::ast_json::to_json(&carve::ast_json::from_json(&json).expect("decodes"));

    assert_eq!(back, json);
}

#[test]
fn an_inline_footnote_keeps_its_number() {
    // It carries its own body, so no deletion can orphan it.
    let json = published("a ^[note] b\n");
    assert!(json.contains("\"number\":1"), "{json}");

    let back = carve::ast_json::to_json(&carve::ast_json::from_json(&json).expect("decodes"));
    assert_eq!(back, json);
}

/// Two numbered figures, with the FIRST removed. Removing the figure a caption
/// belongs to takes the caption with it and proves nothing; the survivor is the
/// one whose number goes stale.
fn captions_without_the_first_figure() -> String {
    let json = published("![a](/1.png)\n^ Figure #: one\n\n![b](/2.png)\n^ Figure #: two\n");
    let start = json
        .find("{\"type\":\"figure\"")
        .expect("the fixture has a figure to remove");
    let bytes = json.as_bytes();
    let mut depth = 0usize;
    let mut end = start;
    for (i, b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut out = String::with_capacity(json.len());
    out.push_str(&json[..start]);
    out.push_str(&json[end..]);

    out.replace(",]", "]").replace("[,", "[").replace(",,", ",")
}

#[test]
fn a_caption_number_is_re_derived_on_ingest() {
    // The survivor is the only figure in the document, so it is Figure 1 - and
    // was published as 2, the number it had when the other figure existed.
    let doc = carve::ast_json::from_json(&captions_without_the_first_figure()).expect("decodes");
    let out = carve::ast_json::to_json(&doc);

    assert!(
        out.contains("\"n\":1"),
        "caption kept a stale number: {out}"
    );
    assert!(!out.contains("\"n\":2"), "{out}");
}

#[test]
fn the_rendered_caption_agrees_with_the_tree() {
    // The half that makes this a defect rather than a wire detail: the stale
    // number was PRINTED, and `carve --from-json` is where a user meets it.
    //
    // Through the BINARY, because there is no public entry point taking a
    // Document and returning HTML - the render module is private, so this path
    // is only reachable the way a caller reaches it.
    let html = run_cli(&["--from-json"], &captions_without_the_first_figure());

    assert!(html.contains("Figure 1: two"), "{html}");
    assert!(!html.contains("Figure 2"), "{html}");
}

#[test]
fn an_unedited_document_keeps_its_caption_numbers() {
    // §6, and the boundary: the pass runs on every ingest, so a tree nobody
    // edited must come back unchanged - both numbers, in order.
    let json = published("![a](/1.png)\n^ Figure #: one\n\n![b](/2.png)\n^ Figure #: two\n");
    let back = carve::ast_json::to_json(&carve::ast_json::from_json(&json).expect("decodes"));

    assert_eq!(back, json);
}

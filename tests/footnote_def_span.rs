//! A footnote definition's span covers its whole body.
//!
//! The published node took the FIRST block's span and stopped there, so a
//! footnote whose body is more than one block - joined by a `+` continuation,
//! or indented under the definition - left every later block outside its own
//! parent (carve#565).

fn json(source: &str) -> String {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    carve::to_json(&carve::parse_with_options(source, &options))
}

/// The `[start, end]` of the LAST `"pos"` in the document, which is the
/// footnote's own: children are written before `pos`, and the footnote
/// definitions are written last.
fn last_pos(json: &str) -> (usize, usize) {
    let at = json.rfind("\"pos\"").expect("no pos in the document");
    let tail = &json[at..];
    let read = |key: &str| -> usize {
        let at = tail.find(key).unwrap_or_else(|| panic!("no {key}")) + key.len();
        let end = tail[at..].find(|c: char| !c.is_ascii_digit()).unwrap();
        tail[at..at + end].parse().unwrap()
    };
    (read("\"startOffset\":"), read("\"endOffset\":"))
}

#[test]
fn a_footnote_span_reaches_the_end_of_its_last_block() {
    let source = "See.[^n]\n\n[^n]: First paragraph.\n+\nSecond paragraph.\n";
    let out = json(source);
    let (fn_start, fn_end) = last_pos(&out);

    assert!(fn_end > fn_start, "footnote span is empty");
    assert!(
        fn_end >= source.find("Second paragraph.").unwrap(),
        "footnote span [{fn_start}, {fn_end}] stops before its second block",
    );
}

#[test]
fn a_single_block_footnote_is_unchanged() {
    let source = "See.[^n]\n\n[^n]: Only one.\n";
    let out = json(source);
    let (start, end) = last_pos(&out);

    assert!(end > start);
    assert!(end <= source.len());
}

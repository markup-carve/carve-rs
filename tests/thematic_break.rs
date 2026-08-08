//! A thematic break (spec §262) is a col-0 run of 3+ of the SAME `-`/`*`/`_`,
//! CONTIGUOUS (no internal spaces) and followed only by trailing whitespace.
//! No leading indent. Matches the executable-spec oracle, carve-js and
//! carve-php. Spaced forms (`* * *`) fall through to a list, indented forms
//! (` ***`) to a paragraph; a mixed run (`-*-`) or fewer than three is never a
//! break.

#[test]
fn contiguous_col0_runs_are_breaks() {
    for src in ["---", "***", "___", "----", "*****", "___", "***\t", "*** "] {
        assert_eq!(
            carve::to_html(src),
            "<hr>",
            "{src:?} should be a thematic break"
        );
    }
}

#[test]
fn authored_markers_survive_parse_ingest_and_write() {
    for (marker, source) in [('-', "---"), ('*', "***"), ('_', "___")] {
        let doc = carve::parse(source);
        let json = carve::ast_json::to_json(&doc);
        if marker == '-' {
            assert!(
                !json.contains(r#""marker""#),
                "default marker published in {json}"
            );
        } else {
            assert!(
                json.contains(&format!(r#""marker":"{marker}""#)),
                "authored marker absent from {json}"
            );
        }
        assert_eq!(
            carve::render_carve(&doc).expect("render"),
            format!("{source}\n")
        );
    }

    let authored = r#"{"type":"document","srcByteLength":3,"children":[{"type":"thematic_break","marker":"_"}]}"#;
    let decoded = carve::ast_json::from_json(authored).expect("marker is accepted");
    let republished = carve::ast_json::to_json(&decoded);
    assert!(republished.contains(r#""marker":"_""#));
    assert_eq!(carve::render_carve(&decoded).expect("render"), "___\n");

    let defaulted =
        r#"{"type":"document","srcByteLength":3,"children":[{"type":"thematic_break"}]}"#;
    let decoded = carve::ast_json::from_json(defaulted).expect("absent marker is accepted");
    assert_eq!(carve::render_carve(&decoded).expect("render"), "---\n");

    let invalid = r#"{"type":"document","srcByteLength":3,"children":[{"type":"thematic_break","marker":"x"}]}"#;
    assert!(carve::ast_json::from_json(invalid).is_err());
}

#[test]
fn spaced_runs_are_not_breaks() {
    // Internal spaces disqualify the break: `* * *`/`- - -` parse as a nested
    // list, `_ _ _` as a paragraph.
    assert_eq!(
        carve::to_html("* * *"),
        "<ul>\n  <li>\n    <ul>\n      <li>*</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- - -"),
        "<ul>\n  <li>\n    <ul>\n      <li>-</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(carve::to_html("_ _ _"), "<p>_ _ _</p>");
}

#[test]
fn indented_runs_are_not_breaks() {
    // A leading space/tab disqualifies the break (must start at column 0).
    assert_eq!(carve::to_html(" ***"), "<p>***</p>");
    assert_eq!(carve::to_html("\t***"), "<p>***</p>");
}

#[test]
fn trailing_content_is_not_a_break() {
    assert_eq!(carve::to_html("---x"), "<p>—x</p>");
    assert_eq!(carve::to_html("*** *"), "<p>*** *</p>");
}

#[test]
fn mixed_or_short_runs_are_not_breaks() {
    assert_eq!(carve::to_html("-*-"), "<p>-*-</p>");
    assert_eq!(carve::to_html("- x"), "<ul>\n  <li>x</li>\n</ul>");
    assert_eq!(carve::to_html("**"), "<p>**</p>");
    assert_eq!(carve::to_html("_"), "<p>_</p>");
}

#[test]
fn a_break_interrupts_a_paragraph_and_heading() {
    assert_eq!(carve::to_html("para\n***"), "<p>para</p>\n<hr>");
    assert_eq!(
        carve::to_html("# H\n***"),
        "<section id=\"H\">\n  <h1>H</h1>\n  <hr>\n</section>"
    );
}

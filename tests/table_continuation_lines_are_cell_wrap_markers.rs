//! CARVE-P12-069: a `+` continuation line is a `cell_wrap` marker in the
//! joined cell and a `continuationLines` count on its row, so the Carve
//! writer can put the `+` lines back. Rows ported from carve-js
//! `test/table-continuation-ast.test.ts`.

use carve::{to_html, to_json};

fn rows(source: &str) -> Vec<serde_json::Value> {
    let doc: serde_json::Value = serde_json::from_str(&to_json(&carve::parse(source))).unwrap();
    doc["children"][0]["rows"].as_array().unwrap().clone()
}

fn positioned_rows(source: &str) -> Vec<serde_json::Value> {
    let options = carve::Options::default().with_positions(true);
    let doc: serde_json::Value =
        serde_json::from_str(&to_json(&carve::parse_with_options(source, &options))).unwrap();
    doc["children"][0]["rows"].as_array().unwrap().clone()
}

fn shape(cell: &serde_json::Value) -> String {
    fn walk(node: &serde_json::Value, out: &mut Vec<String>) {
        match node["type"].as_str() {
            Some("text") => out.push(node["value"].as_str().unwrap().to_string()),
            Some("cell_wrap") => out.push(match node["row"].as_u64() {
                Some(row) => format!("<wrap {} row {row}>", node["line"]),
                None => format!("<wrap {}>", node["line"]),
            }),
            Some(kind) => {
                out.push(format!("<{kind}>"));
                for child in node["children"].as_array().into_iter().flatten() {
                    walk(child, out);
                }
                out.push(format!("</{kind}>"));
            }
            None => {}
        }
    }
    let mut out = Vec::new();
    for child in cell["children"].as_array().into_iter().flatten() {
        walk(child, &mut out);
    }
    out.join(" ")
}

fn writes_back(source: &str) {
    assert_eq!(carve::to_carve(source), source, "{source:?}");
}

#[test]
fn continuation_wrap_points_to_its_join_marker() {
    let source = "| a | b |\n+ c | d |\n";
    let rows = positioned_rows(source);
    assert_eq!(
        rows[0]["cells"][0]["children"][1]["pos"],
        serde_json::json!({
            "startLine": 2, "endLine": 2, "startColumn": 1, "endColumn": 2,
            "startOffset": 10, "endOffset": 11
        })
    );
}

#[test]
fn a_later_continuation_wrap_points_to_its_own_marker() {
    let source = "| a | b |\n+ c | d |\n+ e | f |\n";
    let rows = positioned_rows(source);
    assert_eq!(
        rows[0]["cells"][0]["children"][3]["pos"],
        serde_json::json!({
            "startLine": 3, "endLine": 3, "startColumn": 1, "endColumn": 2,
            "startOffset": 20, "endOffset": 21
        })
    );
}

#[test]
fn a_wrap_moved_to_an_earlier_row_has_no_source_span() {
    let rows = positioned_rows("| a | b |\n| ^ | c |\n+ more | d |\n");
    assert!(rows[0]["cells"][0]["children"][1].get("pos").is_none());
    assert_eq!(rows[1]["cells"][1]["children"][1]["pos"]["startLine"], 3);
}

#[test]
fn a_second_join_on_one_line_points_to_the_pipe() {
    let rows = positioned_rows("| a | < |\n+ x | y |\n");
    let children = rows[0]["cells"][0]["children"].as_array().unwrap();
    let wraps: Vec<_> = children
        .iter()
        .filter(|child| child["type"] == "cell_wrap")
        .collect();
    assert_eq!(wraps[0]["pos"]["startColumn"], 1);
    assert_eq!(wraps[1]["pos"]["startColumn"], 5);
}

#[test]
fn a_wrap_sits_inside_a_span_the_two_lines_share() {
    let source = "| *bold | x |\n+ text* | y |\n";
    let rows = rows(source);
    assert_eq!(rows[0]["continuationLines"], 1);
    assert_eq!(
        shape(&rows[0]["cells"][0]),
        "<strong> bold <wrap 1> text </strong>"
    );
    assert_eq!(shape(&rows[0]["cells"][1]), "x <wrap 1> y");
    writes_back(source);
    assert!(to_html(source).contains("<strong>bold text</strong>"));
}

#[test]
fn an_empty_continuation_line_is_counted() {
    let source = "| a | b |\n+ | |\n";
    assert_eq!(rows(source)[0]["continuationLines"], 1);
    writes_back(source);
}

#[test]
fn a_join_into_the_cell_above_a_rowspan_names_its_row() {
    let source = "| a | b |\n| ^ | c |\n+ more | d |\n";
    let rows = rows(source);
    assert!(rows[0].get("continuationLines").is_none());
    assert_eq!(rows[1]["continuationLines"], 1);
    assert_eq!(shape(&rows[0]["cells"][0]), "a <wrap 1 row 1> more");
    writes_back(source);
}

#[test]
fn a_colspan_marker_keeps_both_fragments() {
    let source = "| a | < |\n+ x | y |\n";
    writes_back(source);
    assert!(to_html(source).contains("a x y"));
}

#[test]
fn a_header_continuation_stays_before_its_separator() {
    let source = "| a | b |\n+ x | y |\n|---|---|\n| e | f |\n";
    assert_eq!(
        carve::to_carve(source),
        "|= a |= b |\n+ x | y |\n| e | f |\n"
    );
}

#[test]
fn a_code_span_across_the_join_holds_no_sentinel() {
    let source = "| `alpha | x |\n+ beta` | y |\n";
    assert!(!to_html(source).contains('\u{2}'));
    assert!(!carve::to_carve(source).contains('\u{2}'));
}

#[test]
fn whitespace_sensitive_inlines_read_the_join_as_a_space() {
    for (source, cell) in [
        ("| a * |\n+ b* | x |\n", "<td>a * b*</td>"),
        ("| a \\ |\n+ b | x |\n", "<td>a &nbsp;b</td>"),
        ("| a -- |\n+ b |\n", "<td>a – b</td>"),
        ("| a | b |\n+ \"b\" | y |\n", "<td>a “b”</td>"),
        ("| a | b |\n+ {.c}x | y |\n", "<td>a {.c}x</td>"),
    ] {
        assert!(
            to_html(source).contains(cell),
            "{source:?}: {}",
            to_html(source)
        );
    }
}

#[test]
fn the_markers_survive_the_wire() {
    let source = "| a | b |\n| ^ | c |\n+ more | d |\n";
    let doc = carve::parse(source);
    assert_eq!(
        carve::from_json(&to_json(&doc)).unwrap().children,
        doc.children
    );
}

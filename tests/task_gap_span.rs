//! A nested list opened after a task checkbox starts at its own marker.

use carve::{from_json, parse_with_options, render_html, to_html, to_json, Options};
use serde_json::Value;

#[test]
fn same_line_task_gap_keeps_nested_list_and_item_spans_at_the_marker() {
    for (source, start) in [
        ("- [?]  - :: t\n         : a\n", 7),
        (
            "- [x]   - :: term\n           : desc\n           :: next\n           : body\n",
            8,
        ),
    ] {
        let parsed = parse_with_options(source, &Options::default().with_positions(true));
        let json = to_json(&parsed);
        let value: Value = serde_json::from_str(&json).unwrap();
        let inner = &value["children"][0]["items"][0]["children"][0];
        assert_eq!(inner["type"], "list");
        assert_eq!(inner["pos"]["startOffset"], start);
        assert_eq!(inner["items"][0]["pos"]["startOffset"], start);
        assert_eq!(inner["pos"]["startColumn"], start + 1);
        assert_eq!(inner["items"][0]["pos"]["startColumn"], start + 1);
        let decoded = from_json(&json).unwrap();
        assert_eq!(to_json(&decoded), json);
        assert_eq!(render_html(&decoded).unwrap(), to_html(source));
    }
}

#[test]
fn task_gap_span_variants_start_at_the_nested_marker() {
    let states = [' ', 'x', '?', '>', '-'];
    for n in 0..1000 {
        let gap = " ".repeat(1 + n % 4);
        let state = states[n % states.len()];
        let source = format!(
            "- [{state}]{gap}- :: term{n}\n{:width$}: body{n}\n",
            "",
            width = 7 + gap.len()
        );
        let expected = source.find("- :: term").expect("nested marker");
        let parsed = parse_with_options(&source, &Options::default().with_positions(true));
        let value: Value = serde_json::from_str(&to_json(&parsed)).unwrap();
        let inner = &value["children"][0]["items"][0]["children"][0];
        assert_eq!(
            inner["pos"]["startOffset"], expected,
            "variant {n}: {source:?}"
        );
        assert_eq!(
            inner["items"][0]["pos"]["startOffset"], expected,
            "variant {n}: {source:?}"
        );
    }
}

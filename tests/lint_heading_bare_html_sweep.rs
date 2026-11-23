use carve::lint::{LintOptions, lint};
use serde_json::Value;

fn positions(input: &str) -> Vec<(u64, u64)> {
    lint(input, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == "bare-html-tag")
        .filter_map(|finding| {
            finding
                .pos
                .map(|p| (u64::from(p.start_offset), u64::from(p.end_offset)))
        })
        .collect()
}

/// The ranges carve-js reports, written by `tools/heading_bare_html_golden.mjs`.
fn golden() -> Vec<(String, Vec<(u64, u64)>)> {
    let text = include_str!("data/heading-bare-html-carve-js.json");
    let file: Value = serde_json::from_str(text).unwrap();
    file["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let ranges = case[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| (r[0].as_u64().unwrap(), r[1].as_u64().unwrap()))
                .collect();
            (case[0].as_str().unwrap().to_string(), ranges)
        })
        .collect()
}

#[test]
fn heading_tag_matches_carve_js_across_4752_variants() {
    let golden = golden();
    let mut checked = 0;
    for level in 1..=6 {
        for quote in ["", "> ", "> > ", "- ", "> - ", "::: note\n"] {
            for before in [
                "",
                "a ",
                "word ",
                "\\",
                "`code` ",
                "<bad ",
                "\\<x> ",
                "<!-- <x> --> ",
                "\\<!-- ",
                "`<!--` ",
                "<xé> ",
            ] {
                for tag in [
                    "<x>",
                    "</x>",
                    "<x/>",
                    "<x a=\"1\">",
                    "<b>",
                    "<x:y>",
                    "<u>",
                    "<em>",
                    "<a href=\"x\">",
                    "<x-1>",
                    "<X>",
                    "<tag data-x=\"v\">",
                ] {
                    let content = format!("{}{} {}{} c\n", quote, "#".repeat(level), before, tag);
                    let input = if quote == "::: note\n" {
                        format!("{content}:::\n")
                    } else {
                        content
                    };
                    let (expected_input, expected) = &golden[checked];
                    assert_eq!(&input, expected_input, "fixture out of step at {checked}");
                    assert_eq!(&positions(&input), expected, "{input:?}");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 4752);
    assert_eq!(golden.len(), checked);
}

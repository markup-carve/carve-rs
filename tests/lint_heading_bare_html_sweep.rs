use carve::lint::{LintOptions, lint};

fn positions(input: &str) -> Vec<(u32, u32)> {
    lint(input, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == "bare-html-tag")
        .filter_map(|finding| finding.pos.map(|p| (p.start_offset, p.end_offset)))
        .collect()
}

#[test]
fn heading_tag_hand_and_data_agree_across_4752_variants() {
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
                    unsafe { std::env::remove_var("CARVE_LINT_RULES") };
                    let hand = positions(&input);
                    unsafe { std::env::set_var("CARVE_LINT_RULES", "data") };
                    let data = positions(&input);
                    assert_eq!(hand, data, "{input:?}");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 4752);
}

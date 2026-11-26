use carve::lint::{LintOptions, lint};

fn spans(source: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == "list-item-block-overindented")
        .filter_map(|finding| finding.pos.map(|p| (p.start_offset, p.end_offset)))
        .collect()
}

#[test]
fn attributed_paragraphs_past_the_list_content_column_are_reported() {
    for marker in ["-", "1."] {
        for attrs in ["{.c}", "{#id}", "{#id .c k=v}"] {
            let source = format!("{marker} a\n\n    {attrs}\n  b\n");
            let start = source.find(attrs).unwrap();
            assert_eq!(
                spans(&source),
                vec![(
                    u32::try_from(start).unwrap(),
                    u32::try_from(start + attrs.len()).unwrap()
                )],
                "{source}"
            );
        }
    }
}

#[test]
fn attributed_paragraph_at_the_content_column_is_not_reported() {
    assert!(spans("- a\n\n  {.c}\n  b\n").is_empty());
}

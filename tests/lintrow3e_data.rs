use carve::lint::{LintOptions, lint};

fn ranges(source: &str, code: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|f| f.code == code)
        .filter_map(|f| f.pos.map(|p| (p.start_offset, p.end_offset)))
        .collect()
}

#[test]
fn corrected_rows_follow_the_pinned_spec() {
    unsafe { std::env::set_var("CARVE_LINT_RULES", "data") };
    let equal = "{table header-rows=0 body-rows=0,2}\n```csv\na,b\nc,d\n```\n";
    assert!(ranges(equal, "delimited-table-wide-row").is_empty());
    let wide = "{table header-rows=0 body-rows=0,2}\n```csv\na,b\nc,d,e\n```\n";
    assert_eq!(ranges(wide, "delimited-table-wide-row").len(), 1);
    assert!(ranges(">[!x]: > [!NOTE]\n", "github-alert-marker").is_empty());
    assert_eq!(ranges(">[!NOTE]\n", "github-alert-marker"), vec![(1, 8)]);
    assert_eq!(
        ranges(">\tx\n", "blockquote-marker-without-space"),
        vec![(0, 3)]
    );
    assert!(ranges("*a\n{_x_}*\n", "block-marker-as-text").is_empty());
}

#[test]
fn corrected_data_rows_hold_across_5000_values() {
    unsafe { std::env::set_var("CARVE_LINT_RULES", "data") };
    for n in 0..1000 {
        let word = format!("v{n}");
        let equal = format!("{{table header-rows=0 body-rows=0,2}}\n```csv\n{word},b\nc,d\n```\n");
        assert!(ranges(&equal, "delimited-table-wide-row").is_empty());
        let wide = format!("{{table header-rows=0 body-rows=0,2}}\n```csv\n{word},b\nc,d,e\n```\n");
        assert_eq!(ranges(&wide, "delimited-table-wide-row").len(), 1);
        assert!(ranges(&format!(">[!x{n}]: > [!NOTE]\n"), "github-alert-marker").is_empty());
        let quote = format!(">\t{word}\n");
        assert_eq!(
            ranges(&quote, "blockquote-marker-without-space"),
            vec![(
                0,
                u32::try_from(quote.trim_end().len()).expect("short test input")
            )]
        );
        let span = format!("*{word}\n{{_{word}_}}*\n");
        assert!(ranges(&span, "block-marker-as-text").is_empty());
    }
}

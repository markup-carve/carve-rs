use carve::{Options, SmartTypographyMode};

const SOURCE: &str = "He said \"hi\" and 'yes'; it's fine... a--b c---d -> <= (c) (r) (tm) +-\n";
const EXPECTED: &str = "He said \"hi\" and 'yes'; it's fine… a–b c—d → ≤ © ® ™ ±";

#[test]
fn quotes_only_typography_reaches_every_target() {
    let options = Options {
        smart_typography: SmartTypographyMode::QuotesSource,
        ..Options::default()
    };
    for output in [
        carve::to_html_with_options(SOURCE, &options),
        carve::to_markdown_with_options(SOURCE, &options),
        carve::to_plain_text_with_options(SOURCE, &options),
        carve::to_ansi_with_options(SOURCE, &options),
    ] {
        assert!(output.contains(EXPECTED), "{output}");
    }
}

#[test]
fn quotes_only_typography_preserves_literals_and_heading_ids() {
    let options = Options {
        smart_typography: SmartTypographyMode::QuotesSource,
        ..Options::default()
    };
    let input = "“typed” ‘quotes’ \\\"escaped\\\" \\'single\\' and `a--b \"q\"`\n\n# Don't \"guess\"... a--b\n\n</#Don-t-guess-a-b>\n";
    for output in [
        carve::to_html_with_options(input, &options),
        carve::to_markdown_with_options(input, &options),
        carve::to_plain_text_with_options(input, &options),
        carve::to_ansi_with_options(input, &options),
    ] {
        assert!(
            output
                .replace('\\', "")
                .contains("“typed” ‘quotes’ \"escaped\" 'single'"),
            "{output}"
        );
        assert!(output.contains("a--b \"q\""), "{output}");
        assert_eq!(
            output.matches("Don't \"guess\"… a–b").count(),
            2,
            "{output}"
        );
    }
    assert!(carve::to_html_with_options(input, &options).contains("id=\"Don-t-guess-a-b\""));
    assert!(carve::to_html(input).contains("id=\"Don-t-guess-a-b\""));
}

#[test]
fn accessible_task_names_and_tab_labels_use_quotes_only_typography() {
    let tabs = carve::Tabs::new();
    let options = Options {
        smart_typography: SmartTypographyMode::QuotesSource,
        ..Options::default()
    }
    .with_extension(&tabs);
    assert!(
        carve::to_html_with_options("- [ ] Don't \"guess\"... a--b\n", &options)
            .contains("aria-label=\"Don&apos;t &quot;guess&quot;… a–b\"")
    );
    let input = ":::: tabs\n::: tab\n# Don't \"guess\"... a--b\n\nBody.\n:::\n::::\n";
    assert!(carve::to_html_with_options(input, &options).contains(">Don't \"guess\"… a–b</label>"));
}

#[test]
fn index_backlink_names_use_quote_source_runs() {
    let index = carve::Index::new();
    let options = Options {
        smart_typography: SmartTypographyMode::QuotesSource,
        ..Options::default()
    }
    .with_extension(&index);
    let output =
        carve::to_html_with_options(":index[\"quoted\" term]\n\n::: index\n:::\n", &options);
    assert!(output.contains("&quot;quoted&quot; term"), "{output}");
    assert!(!output.contains('“') && !output.contains('”'), "{output}");
}

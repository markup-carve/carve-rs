use carve::lint::{LintOptions, lint};

fn codes(source: &str) -> Vec<&'static str> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.channel == "lint")
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn caption_hint_reads_paragraph_text_behind_description_marker() {
    for source in [
        ":: term4\n: body\n    |= A |= B |\n",
        ":: term\n: Caption\n    |= A |= B |\n",
    ] {
        assert!(
            !codes(source).contains(&"caption-looking-paragraph"),
            "{source:?}"
        );
    }
    assert!(codes(": Caption\n|= A |= B |\n").contains(&"caption-looking-paragraph"));
}

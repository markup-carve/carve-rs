use carve::{HeadingNumbers, HeadingNumbersOptions, Options, TocPlacement};

#[test]
fn toc_entry_text_excludes_section_number() {
    let hn = HeadingNumbers::with_options(HeadingNumbersOptions::default());
    let toc = TocPlacement::new();
    let opts = Options::new().with_extension(&hn).with_extension(&toc);
    let out = carve::to_html_with_options("::: toc\n:::\n\n# Alpha\n\n## Beta\n", &opts);
    assert!(out.contains("<a href=\"#Alpha\">Alpha</a>"), "{out}");
    assert!(!out.contains(">1 Alpha<"), "{out}");
}

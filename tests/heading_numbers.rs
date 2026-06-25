//! HeadingNumbers (#198) — byte-parity with the carve-js reference.

use carve::{CrossrefStyle, HeadingNumbers, HeadingNumbersOptions, Options};

fn h(src: &str) -> String {
    let ext = HeadingNumbers::new();
    let opts = Options::new().with_extension(&ext);
    carve::to_html_with_options(src, &opts).trim().to_string()
}

fn h_opts(src: &str, o: HeadingNumbersOptions) -> String {
    let ext = HeadingNumbers::with_options(o);
    let opts = Options::new().with_extension(&ext);
    carve::to_html_with_options(src, &opts).trim().to_string()
}

#[test]
fn numbers_dotted_per_level() {
    let out = h("# A\n\n## B\n\n## C\n\n### D");
    assert!(out.contains("<span class=\"section-number\">1</span> A"));
    assert!(out.contains("<span class=\"section-number\">1.1</span> B"));
    assert!(out.contains("<span class=\"section-number\">1.2</span> C"));
    assert!(out.contains("<span class=\"section-number\">1.2.1</span> D"));
}

#[test]
fn min_level_starts_deeper() {
    let o = HeadingNumbersOptions {
        min_level: 2,
        ..Default::default()
    };
    let out = h_opts("# Title\n\n## First\n\n### Sub\n\n## Second", o);
    assert!(out.contains("<h1>Title</h1>"));
    assert!(out.contains("<span class=\"section-number\">1</span> First"));
    assert!(out.contains("<span class=\"section-number\">1.1</span> Sub"));
    assert!(out.contains("<span class=\"section-number\">2</span> Second"));
}

#[test]
fn resets_deeper_counters() {
    let out = h("# A\n\n## A1\n\n# B\n\n## B1");
    assert!(out.contains("<span class=\"section-number\">1.1</span> A1"));
    assert!(out.contains("<span class=\"section-number\">2</span> B"));
    assert!(out.contains("<span class=\"section-number\">2.1</span> B1"));
}

#[test]
fn gap_free_across_skipped_levels() {
    let out = h("# A\n\n### C");
    assert!(out.contains("<span class=\"section-number\">1</span> A"));
    assert!(out.contains("<span class=\"section-number\">1.1</span> C"));
    assert!(!out.contains("1.0"));
}

#[test]
fn first_id_wins_even_when_skipped() {
    let out = h("{#dup .unnumbered}\n# First\n\n{#dup}\n# Second\n\nSee </#dup>.");
    assert!(!out.contains("Section 2 - Second"));
    assert!(!out.contains("Section 1 - Second"));
}

#[test]
fn unnumbered_skips_and_does_not_advance() {
    let out = h("# A\n\n{.unnumbered}\n# Preface\n\n# B");
    assert!(out.contains("<span class=\"section-number\">1</span> A"));
    assert!(!out.contains("</span> Preface"));
    assert!(out.contains("<span class=\"section-number\">2</span> B"));
}

#[test]
fn does_not_crash_on_figure() {
    let out = h("# A\n\n![alt](/img.png)\n^ A caption.\n\n## B");
    assert!(out.contains("<span class=\"section-number\">1</span> A"));
    assert!(out.contains("<span class=\"section-number\">1.1</span> B"));
}

#[test]
fn does_not_number_blockquote_headings() {
    let out = h("# A\n\n> # Quoted");
    assert!(out.contains("<span class=\"section-number\">1</span> A"));
    assert!(!out.contains("section-number\">1.1"));
    assert!(!out.contains("</span> Quoted"));
}

#[test]
fn full_golden_matches_carve_js() {
    let o = HeadingNumbersOptions {
        min_level: 2,
        ..Default::default()
    };
    let out = h_opts(
        "# Title\n\n## Parsing\n\nSee </#Parsing> and </#Rendering>.\n\n### Tokens\n\n## Rendering\n\n{.unnumbered}\n## Changelog",
        o,
    );
    let expected = "<section id=\"Title\">\n  <h1>Title</h1>\n  <section id=\"Parsing\">\n    <h2><span class=\"section-number\">1</span> Parsing</h2>\n    <p>See <a href=\"#Parsing\">Section 1 - Parsing</a> and <a href=\"#Rendering\">Section 2 - Rendering</a>.</p>\n    <section id=\"Tokens\">\n      <h3><span class=\"section-number\">1.1</span> Tokens</h3>\n    </section>\n  </section>\n  <section id=\"Rendering\">\n    <h2><span class=\"section-number\">2</span> Rendering</h2>\n  </section>\n  <section id=\"Changelog\">\n    <h2 class=\"unnumbered\">Changelog</h2>\n  </section>\n</section>";
    assert_eq!(out, expected);
}

#[test]
fn crossref_default_number_title() {
    assert!(
        h("# Parsing\n\nSee </#Parsing>.").contains("<a href=\"#Parsing\">Section 1 - Parsing</a>")
    );
}

#[test]
fn crossref_number_only() {
    let o = HeadingNumbersOptions {
        crossref: CrossrefStyle::Number,
        ..Default::default()
    };
    assert!(
        h_opts("# Parsing\n\nSee </#Parsing>.", o).contains("<a href=\"#Parsing\">Section 1</a>")
    );
}

#[test]
fn crossref_title_leaves_refs() {
    let o = HeadingNumbersOptions {
        crossref: CrossrefStyle::Title,
        ..Default::default()
    };
    let out = h_opts("# Parsing\n\nSee </#Parsing>.", o);
    assert!(out.contains("<a href=\"#Parsing\">Parsing</a>"));
    assert!(out.contains("<span class=\"section-number\">1</span> Parsing"));
}

#[test]
fn label_is_configurable() {
    let o = HeadingNumbersOptions {
        label: "§".to_string(),
        ..Default::default()
    };
    assert!(h_opts("# Parsing\n\nSee </#Parsing>.", o)
        .contains("<a href=\"#Parsing\">§ 1 - Parsing</a>"));
}

#[test]
fn leaves_explicit_text_link() {
    let out = h("# Parsing\n\n[my words](#Parsing).");
    assert!(out.contains("<a href=\"#Parsing\">my words</a>"));
}

#[test]
fn leaves_explicit_same_title_link() {
    let out = h("# Parsing\n\n[Parsing](#Parsing).");
    assert!(out.contains("<a href=\"#Parsing\">Parsing</a>"));
    assert!(!out.contains("Section 1 - Parsing"));
}

#[test]
fn leaves_implicit_reference() {
    let out = h("# Parsing\n\nSee [Parsing][].");
    assert!(out.contains(">Parsing</a>"));
    assert!(!out.contains("Section 1 - Parsing"));
}

#[test]
fn first_heading_for_duplicate_id() {
    let out = h("{#dup}\n# First\n\n{#dup}\n# Second\n\nSee </#dup>.");
    assert!(out.contains("Section 1 - First"));
    assert!(!out.contains("Section 2 - Second"));
}

#[test]
fn does_not_rewrite_link_to_unnumbered() {
    let out = h("{.unnumbered}\n# Notes\n\n[Notes](#Notes).");
    assert!(out.contains("<a href=\"#Notes\">Notes</a>"));
}

#[test]
fn degradation_without_extension() {
    let out = carve::to_html("# Parsing\n\nSee </#Parsing>.")
        .trim()
        .to_string();
    assert!(!out.contains("section-number"));
    assert!(out.contains("<a href=\"#Parsing\">Parsing</a>"));
}

//! cmark-gfm has no dollar math, so a Markdown import keeps `$...$` as text
//! (corpus-convert 08). Only the roundtrip caption marker restores an equation.

use carve::Options;

fn html(markdown: &str) -> String {
    carve::to_html(&carve::migrate_markdown(markdown).value)
}

#[test]
fn the_shared_converter_case_keeps_dollar_text() {
    let case = "spec/tests/corpus-convert/08-markdown-superscript-highlight-and-math-stay-text";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(case);
    let input = std::fs::read_to_string(root.join("input.md")).unwrap();
    let expected = std::fs::read_to_string(root.join("expected.html")).unwrap();
    assert_eq!(html(&input).trim(), expected.trim());
}

#[test]
fn dollar_delimited_text_imports_as_text() {
    for (markdown, expected) in [
        ("g $x+y$ h\n", "<p>g $x+y$ h</p>"),
        ("$$x+y$$\n", "<p>$$x+y$$</p>"),
        ("$$\nx+y\n$$\n", "<p>$$\nx+y\n$$</p>"),
        ("- costs $5 and $6\n", "<li>costs $5 and $6</li>"),
        ("| a |\n| - |\n| $x$ |\n", "<td>$x$</td>"),
    ] {
        let rendered = html(markdown);
        assert!(
            !rendered.contains("class=\"math"),
            "{markdown} => {rendered}"
        );
        assert!(rendered.contains(expected), "{markdown} => {rendered}");
    }
}

#[test]
fn dollar_text_beside_a_carrier_marker_stays_text() {
    let markdown = "<!-- carve: {.note} -->\nprice $x+y$ here\n";
    let imported = carve::migrate_markdown(markdown);
    assert_eq!(imported.value, "{.note}\nprice $x+y$ here\n");
    assert!(!carve::to_html(&imported.value).contains("class=\"math"));
}

#[test]
fn a_captioned_equation_still_comes_back_through_its_marker() {
    let source = "$$`x+y`\n^ Equation caption\n";
    let exported =
        carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    let imported = carve::migrate_markdown(&exported);
    assert_eq!(imported.value, source, "{exported}");
    assert_eq!(carve::to_html(&imported.value), carve::to_html(source));
}

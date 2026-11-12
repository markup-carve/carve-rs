//! Markdown import follows the carve#2254 importer rule (markup-carve/carve#6010):
//! a destination the PART 9 §25 sink denylist blanks is not written, only the
//! content is, with one `attribute-dropped` warning per dropped destination.

use carve::{markdown_to_carve, migrate_markdown};

const LINK_ROW: &str = "Dropped a link destination with a denied URL scheme";
const IMAGE_ROW: &str = "Dropped an image source with a denied URL scheme";
const REFERENCE_ROW: &str = "Dropped a reference definition with a denied URL scheme";

fn written(markdown: &str) -> String {
    markdown_to_carve(markdown)
        .trim_end_matches('\n')
        .to_owned()
}

fn dropped(markdown: &str) -> Vec<(String, String, String, String)> {
    migrate_markdown(markdown)
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "attribute-dropped")
        .map(|row| {
            (
                row.severity.as_str().to_owned(),
                row.fidelity.as_str().to_owned(),
                row.confidence.as_str().to_owned(),
                row.message.clone(),
            )
        })
        .collect()
}

fn warning(message: &str) -> (String, String, String, String) {
    (
        "warning".to_owned(),
        "dropped".to_owned(),
        "exact".to_owned(),
        message.to_owned(),
    )
}

#[test]
fn the_reproduction_imports_as_its_content_with_one_warning_per_destination() {
    let markdown = "Siehe [x](javascript:alert(1)) und ![b](data:image/png;base64,AA).\n";
    let result = migrate_markdown(markdown);
    assert_eq!(result.value, "Siehe x und b.\n");
    assert_eq!(markdown_to_carve(markdown), "Siehe x und b.\n");
    assert_eq!(
        dropped(markdown),
        vec![warning(LINK_ROW), warning(IMAGE_ROW)]
    );
}

#[test]
fn every_destination_carrier_is_dropped_to_its_content() {
    for (name, markdown, carve) in [
        ("scheme case", "[x](JaVaScRiPt:x)", "x"),
        (
            "a title goes with the destination",
            "[x](vbscript:x \"t\")",
            "x",
        ),
        ("an entity-encoded scheme", "[x](&#106;avascript:x)", "x"),
        ("an OS-handler scheme", "[x](ms-msdt:x)", "x"),
        (
            "link text keeps its markup",
            "[*a* `c`](javascript:x)",
            "/a/ `c`",
        ),
        ("a pointy destination", "[x](<javascript:x>)", "x"),
        (
            "an autolink keeps its text",
            "<javascript:alert(1)>",
            "javascript:alert(1)",
        ),
        (
            "a block image becomes its alt text",
            "![a b](data:x)",
            "a b",
        ),
        ("a titled block image", "![a *b*](data:x \"t\")", "a b"),
        (
            "an empty block image leaves nothing",
            "p\n\n![](data:x)",
            "p",
        ),
        (
            "an image inside an allowed link",
            "[![i](data:x)](https://e)",
            "[i](https://e)",
        ),
        (
            "an allowed image inside a denied link",
            "[![i](https://e/i.png)](javascript:x)",
            "![i](https://e/i.png)",
        ),
        (
            "a reference link and its definition",
            "[x][r] ![i][r]\n\n[r]: javascript:x",
            "x i",
        ),
        ("an unused definition", "p\n\n[r]: data:x", "p"),
        (
            "an empty link inside emphasis",
            "a *[](javascript:x)* b",
            "a  b",
        ),
        (
            "in a definition description",
            "Term\n: [y](javascript:x) d",
            "Term\n: y d",
        ),
        ("in a quoted list item", "> - [y](javascript:x)", "> - y"),
        (
            "in a table cell",
            "| a |\n|---|\n| [](javascript:x) [ok](https://e) |",
            "|= a |\n| [ok](https://e) |",
        ),
        (
            "in a footnote",
            "a[^1]\n\n[^1]: see [x](javascript:x)",
            "a[^1]\n\n[^1]: see x",
        ),
    ] {
        assert_eq!(written(markdown), carve, "{name}");
    }
}

#[test]
fn a_reference_definition_is_reported_once_for_every_use() {
    assert_eq!(
        dropped("[x][r] [y][r] ![i][r]\n\n[r]: javascript:x\n"),
        vec![warning(REFERENCE_ROW)]
    );
    assert_eq!(dropped("p\n\n[r]: data:x\n"), vec![warning(REFERENCE_ROW)]);
}

#[test]
fn a_percent_escaped_destination_is_not_a_denied_scheme() {
    // The written form is what the sink reads, and `%09` is no scheme.
    assert_eq!(written("[x](java&#x09;script:x)"), "[x](java%09script:x)");
    assert_eq!(dropped("[x](java&#x09;script:x)"), vec![]);
}

#[test]
fn an_allowed_destination_is_kept() {
    for (markdown, carve) in [
        ("[x](https://e)", "[x](https://e)"),
        ("[x](mailto:a@b.c)", "[x](mailto:a@b.c)"),
        ("[x](/rel)", "[x](/rel)"),
        ("[x](javascripts:x)", "[x](javascripts:x)"),
        ("[x](%6Aavascript:x)", "[x](%6Aavascript:x)"),
        ("[x][r]\n\n[r]: https://e", "[x][r]\n\n[r]: https://e"),
        ("p\n\n[r]: https://e", "p\n\n[r]: https://e"),
    ] {
        assert_eq!(written(markdown), carve, "{markdown}");
        assert_eq!(dropped(markdown), vec![], "{markdown}");
    }
}

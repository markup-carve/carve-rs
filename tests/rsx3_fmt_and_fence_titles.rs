//! Writer readback for spans of closing brackets and deep unclosed brackets (#4598, #4599), delimited
//! tables with a foot or body header rows (carve#6423), and lint of code fence titles (#4604).
use carve::api::{Carve, Options, WriteOptions};
use carve::lint::{LintOptions, lint};

fn assert_unchanged(input: &str) {
    let carve = Carve::new();
    assert_eq!(
        carve.format(input, &WriteOptions::default()).unwrap(),
        input,
        "{input:?}"
    );
}

#[test]
fn span_of_closing_brackets_is_returned_unchanged() {
    for input in ["[\\]\\]]{}\n", "[a\\]\\]]{}\n", "[\\]\\]\\]]{.c}\n"] {
        assert_unchanged(input);
    }
}

fn unclosed_brackets(n: usize) -> String {
    let parts = ["[a ", "*[a](u)*", "[x]: /u\n"];
    (0..n).map(|i| parts[i % 3]).collect()
}

#[test]
fn unclosed_brackets_past_the_depth_cap_read_back() {
    assert_unchanged(&unclosed_brackets(6));
    let carve = Carve::new();
    let (write, read) = (WriteOptions::default(), Options::default());
    for n in [711, 900, 2_000, 3_008] {
        let input = unclosed_brackets(n);
        let out = carve.format(&input, &write).unwrap();
        assert_eq!(carve.format(&out, &write).unwrap(), out, "n = {n}");
        assert_eq!(
            carve.render(out.as_str(), &read).unwrap(),
            carve.render(input.as_str(), &read).unwrap(),
            "n = {n}"
        );
    }
}

#[test]
fn delimited_table_with_foot_or_body_header_rows_keeps_its_form() {
    assert_unchanged("{table footer-rows=1}\n```csv\nh\na\n b\n```\n");
    assert_unchanged("{table body-rows=1 body-header-rows=1}\n```csv\nh\na\n b\n```\n");
}

fn findings(source: &str) -> Vec<(&'static str, u32, u32)> {
    let mut found: Vec<_> = lint(source, &LintOptions::default())
        .into_iter()
        .filter(|f| f.code != "unclosed-code-fence" && f.code != "block-opener-overindented")
        .filter_map(|f| f.pos.map(|p| (f.code, p.start_offset, p.end_offset)))
        .collect();
    found.sort_unstable();
    found
}

#[test]
fn code_fence_title_is_no_inline_host() {
    assert_eq!(findings("``` \"[x]{#i}\"\n"), []);
    assert_eq!(
        findings("``` js \"*a*{.c}\"\n"),
        [("emphasis-marker-before-attributes", 10, 15)]
    );
}

#[test]
fn quoted_title_behind_a_second_space_is_further_words() {
    assert_eq!(
        findings(" ```  \"[x](/u)\"\n"),
        [("fence-title-syntax", 6, 15)]
    );
    let tree = Carve::new()
        .parse("```  \"t\" [l]\nx\n```\n", &Options::default())
        .unwrap();
    let html = Carve::new()
        .render_tree(&tree, &Options::default())
        .unwrap();
    assert!(!html.contains("title="), "{html}");
}

#[test]
fn marker_range_ends_at_its_first_attribute_block() {
    assert_eq!(
        findings("a*{.x}{.y} b\n"),
        [
            ("emphasis-marker-before-attributes", 1, 6),
            ("unattached-inline-attribute", 2, 10)
        ]
    );
}

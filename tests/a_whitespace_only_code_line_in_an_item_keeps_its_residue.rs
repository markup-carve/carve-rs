//! A whitespace-only line of a fenced code block inside a list item keeps
//! what lies past the item's content column (PART 11 §7).

use carve::{html_to_carve, parse, to_carve, BlockNode, HtmlImportOptions};

fn code(source: &str) -> String {
    fn find(blocks: &[BlockNode]) -> Option<String> {
        blocks.iter().find_map(|block| match block {
            BlockNode::CodeBlock(code) => Some(code.content.clone()),
            BlockNode::List(list) => list.items.iter().find_map(|item| find(&item.children)),
            BlockNode::BlockQuote(quote) => find(&quote.children),
            _ => None,
        })
    }
    find(&parse(source).children).unwrap_or_else(|| panic!("no code block in {source:?}"))
}

#[test]
fn the_residue_past_the_content_column_is_content() {
    for (source, content) in [
        ("- ```\n  a\n    \n  b\n  ```\n", "a\n  \nb\n"),
        ("- - ```\n    a\n       \n    b\n    ```\n", "a\n   \nb\n"),
        (
            "- a\n  - ```\n    x\n       \n    y\n    ```\n",
            "x\n   \ny\n",
        ),
        ("> - ```\n>   a\n>     \n>   b\n>   ```\n", "a\n  \nb\n"),
    ] {
        assert_eq!(code(source), content, "{source:?}");
    }
}

#[test]
fn a_line_no_wider_than_the_content_column_is_empty() {
    assert_eq!(code("- ```\n  a\n  \n  b\n  ```\n"), "a\n\nb\n");
    assert_eq!(code("- ```\n  a\n \n  b\n  ```\n"), "a\n\nb\n");
}

#[test]
fn an_imported_whitespace_line_is_a_fmt_fixed_point() {
    let written = html_to_carve(
        "<ul><li><pre>a\n \nb</pre></li></ul>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert_eq!(code(&written), "a\n \nb\n");
    assert_eq!(to_carve(&written), written);
}

#[test]
fn a_straddling_tab_keeps_offsets_inside_the_source() {
    let source = "- ```\n  x\n\t";
    let json = carve::to_json(&carve::parse_with_options(
        source,
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    ));
    for offset in json.split("\"endOffset\":").skip(1) {
        let end: usize = offset
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(end <= source.len(), "{json}");
    }
}

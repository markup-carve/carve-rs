//! carve#5197: djot.js charges a blank read inside a nested list to that list,
//! so the outer item stays tight and the import must not loosen it.

use carve::djot_to_carve;

#[test]
fn drops_the_blank_before_a_block_that_ends_the_nested_item() {
    for (source, expected) in [
        ("- one\n\n  - two\n\n  > q\n", "- one\n  - two\n  > q\n"),
        (
            "- one\n\n  - two\n\n  ```\n  c\n  ```\n",
            "- one\n  - two\n  ```\n  c\n  ```\n",
        ),
        ("- one\n\n  - two\n\n  # h\n", "- one\n  - two\n  # h\n"),
        ("- one\n\n  a) two\n\n  > q\n", "- one\n  a) two\n  > q\n"),
    ] {
        assert_eq!(djot_to_carve(source), expected, "{source:?}");
    }
}

#[test]
fn attaches_a_paragraph_after_the_nested_list_with_a_plus() {
    for (source, expected) in [
        ("- one\n\n  - two\n\n  para\n", "- one\n  - two\n+\npara\n"),
        (
            "> - one\n>\n>   - two\n>\n>   para\n",
            "> - one\n>   - two\n> +\n> para\n",
        ),
        (
            "- a\n- one\n\n  - two\n\n  para\n  more\n",
            "- a\n- one\n  - two\n+\npara\n  more\n",
        ),
    ] {
        assert_eq!(djot_to_carve(source), expected, "{source:?}");
    }
}

#[test]
fn an_attribute_line_after_the_nested_list_keeps_the_item_tight() {
    let source = "- one\n\n  1. two\n\n  {.c}\n  para\n";
    assert_eq!(djot_to_carve(source), "- one\n  1. two\n  {.c}\n  para\n");
    // A blank directly after the lead still loosens the item.
    let source = "- one\n\n  {.c}\n  para\n";
    assert_eq!(djot_to_carve(source), "{loose}\n- one\n\n  {.c}\n  para\n");
}

#[test]
fn reads_a_pipe_led_or_brace_led_paragraph_as_a_paragraph() {
    for line in ["| not a table", "{not attributes"] {
        let source = format!("- one\n\n  - two\n\n  {line}\n");
        let html = carve::to_html(&djot_to_carve(&source));
        let compact: String = html.split_whitespace().collect();
        assert!(compact.contains("</ul>") && !compact.contains("<li><p>"), "{source:?} -> {html}");
    }
}

#[test]
fn keeps_a_blank_the_item_owns() {
    for (source, expected) in [
        ("- one\n\n  > q\n", "- one\n\n  > q\n"),
        (
            "- one\n\n  - two\n\n  para\n\n  more\n",
            "- one\n  - two\n\n  para\n\n  more\n",
        ),
        (
            "- one\n\n- b\n\n  - two\n\n  para\n",
            "- one\n\n- b\n  - two\n\n  para\n",
        ),
    ] {
        assert_eq!(djot_to_carve(source), expected, "{source:?}");
    }
}

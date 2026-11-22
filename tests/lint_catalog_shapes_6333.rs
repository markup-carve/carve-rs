//! Hand-written rules follow the corrected catalog rows of carve#6333 (CC-1542, CC-1546, CC-1547, CC-1549).

fn ranges(input: &str, code: &str) -> Vec<(u32, u32)> {
    carve::lint::lint(input, &carve::lint::LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .map(|finding| {
            let pos = finding.pos.expect("range");
            (pos.start_offset, pos.end_offset)
        })
        .collect()
}

#[test]
fn bare_html_tag_needs_the_catalog_tag_shape() {
    for input in [
        "[a <x]> b](u)\n",
        "a <x]> b\n",
        "a <x/y> b\n",
        "a <x\"> b\n",
        "a <x-1:_> b\n",
        "| <h:i> |\n",
    ] {
        assert!(ranges(input, "bare-html-tag").is_empty(), "{input:?}");
    }
    for (input, range) in [
        ("a <x> b\n", (2, 5)),
        ("a <x/> b\n", (2, 6)),
        ("a </x> b\n", (2, 6)),
        ("a <x y=\"]\"> b\n", (2, 11)),
        ("a <x-1:b_c\tz> b\n", (2, 13)),
        ("a <x/y> <b>\n", (8, 11)),
        ("| a <x/y> <b> |\n", (10, 13)),
        ("| a <!-- c --> <x]> <b> |\n", (20, 23)),
    ] {
        assert_eq!(ranges(input, "bare-html-tag"), [range], "{input:?}");
    }
}

#[test]
fn bare_html_tag_in_heading_content() {
    for (input, expected) in [
        ("# a <b> c\n", vec![(4, 7)]),
        ("# <x>\n", vec![(2, 5)]),
        ("## </x>\n", vec![(3, 7)]),
        ("### <x/>\n", vec![(4, 8)]),
        ("# <x a=\"1\">\n", vec![(2, 11)]),
        ("# \\<x>\n", vec![]),
        ("# <!-- <x> -->\n", vec![]),
        ("# <!-- <x> --> <b>\n", vec![(15, 18)]),
        ("# \\<x> <b>\n", vec![(7, 10)]),
        ("# `<!--` <b>\n", vec![(9, 12)]),
        ("# \\<!-- <b>\n", vec![(8, 11)]),
        ("# <xé> <b>\n", vec![(8, 11)]),
    ] {
        assert_eq!(ranges(input, "bare-html-tag"), expected, "{input:?}");
    }
}

#[test]
fn bare_html_tag_after_many_inline_code_candidates() {
    let input = format!("# {}<b>\n", "`<x>` ".repeat(1_000));
    let start = u32::try_from(input.find("<b>").unwrap()).unwrap();
    assert_eq!(ranges(&input, "bare-html-tag"), [(start, start + 3)]);
}

#[test]
fn caption_looking_paragraph_skips_term_text() {
    for input in [
        " :: term\n: | A | B |\n  | x | y |\n",
        "\t:: t\n: ## H #\n | 1 | 2 |\n",
        ":: term\nCaption: x\n| a |\n",
    ] {
        assert!(
            ranges(input, "caption-looking-paragraph").is_empty(),
            "{input:?}"
        );
    }
    assert_eq!(
        ranges("Caption: x\n| a | b |\n", "caption-looking-paragraph"),
        [(0, 1)]
    );
}

#[test]
fn github_alert_marker_skips_a_glued_first_quote_mark() {
    for input in [
        ">[x]: > [!NOTE]\n",
        ">* > [!NOTE]\n",
        ">- > [!NOTE]\n",
        "  >a > [!TIP]\n",
    ] {
        assert!(ranges(input, "github-alert-marker").is_empty(), "{input:?}");
    }
    for (input, start) in [
        ("> [!NOTE]\n", 2),
        (">[!NOTE]\n", 1),
        (">\t[!NOTE]\n", 2),
        ("> [x]: > [!NOTE]\n", 9),
    ] {
        let found = ranges(input, "github-alert-marker");
        assert_eq!(found.len(), 1, "{input:?}");
        assert_eq!(found[0].0, start, "{input:?}");
    }
}

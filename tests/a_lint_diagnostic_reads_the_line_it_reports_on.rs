//! THREE `carve lint` DIAGNOSTICS NOTHING COULD BE DONE ABOUT
//! (markup-carve/carve-rs#2099).
//!
//! `unclosed-container-fence` on a fenced quote whose closer is right there. The
//! rule finds the closer by reading the container node's own extent, and a
//! fenced quote's extent stopped at its last content line.
//!
//! The `table-column-*` rules on a line that configures no table. The pass
//! matched any line holding `aligns=` above a pipe row, and a brace run with
//! text beside it attaches to nothing under PART 9 §15, which makes it paragraph
//! text.
//!
//! A column counted in bytes. A `LintWarning`'s `start` and `end` are byte
//! offsets by design, which the struct states; its `column` is the number a
//! `Pos` carries, and PART 12 §4 counts that in codepoints.
//!
//! Cross-read against carve-js 6c4b882b1 and carve-php 4efa406bc. Both are
//! silent on the two documents the ticket reports. On a column, carve-php agrees
//! exactly; carve-js counts UTF-16 units for a lint offset, which its own
//! documentation states, so it reads one higher than the codepoint column
//! wherever an astral character sits to the left. A case with no astral
//! character is here for the three-way reading.

use carve::{lint_carve, parse_with_options, BlockNode, Options};

fn rules(source: &str) -> Vec<&'static str> {
    lint_carve(source).into_iter().map(|w| w.rule).collect()
}

#[test]
fn a_closed_nested_quote_fence_is_not_reported_as_unclosed() {
    // The reported document: a quote holding a quote, both fences closed.
    let reported = "::: >\nCarol\n::: >\nBob\n:::\n:::\n";
    assert!(
        !rules(reported).contains(&"unclosed-container-fence"),
        "{:?}",
        lint_carve(reported)
    );
    assert!(
        !rules("::: >\nA\n:::\n").contains(&"unclosed-container-fence"),
        "one level deep is the same rule"
    );
}

#[test]
fn a_fenced_quote_spans_its_closer_like_every_other_colon_fence() {
    // The cause, stated on the node rather than on the diagnostic. A div, an
    // admonition and a line block over the same three lines all reach line 3;
    // the fenced quote reached line 2, which is why the linter looked for its
    // closer above it.
    for source in [":::\nA\n:::\n", "::: note\nA\n:::\n", "::: |\nA\n:::\n"] {
        assert_eq!(first_block_span(source), (1, 3), "{source:?}");
    }
    assert_eq!(first_block_span("::: >\nA\n:::\n"), (1, 3));
}

#[test]
fn the_prefix_spelling_still_ends_at_its_content() {
    // THE CONTROL, and the whole reason the two spellings are told apart: a `>`
    // quote has no closer, so its extent is the lines it consumed. Skipping the
    // narrowing for every quote would have moved this.
    assert_eq!(first_block_span("> a\n"), (1, 1));
    assert_eq!(first_block_span("> a\n> b\n"), (1, 2));
}

#[test]
fn an_unclosed_fence_is_still_reported() {
    // THE OTHER CONTROL. The rule has to keep firing, so a fenced quote with no
    // closer at all still reports - otherwise widening the span would have
    // silenced the rule rather than fixed it.
    assert!(rules("::: >\nA\n").contains(&"unclosed-container-fence"));
    assert!(rules(":::\nA\n").contains(&"unclosed-container-fence"));
    assert!(rules("::: >\nCarol\n::: >\nBob\n:::\n").contains(&"unclosed-container-fence"));
}

#[test]
fn a_brace_run_with_text_beside_it_configures_no_table() {
    // The reported document. The first line is a paragraph, so the attribute
    // block attaches to nothing and lists none of the table's column metadata.
    for rule in rules("é😀 {aligns=\"left\"}\n| a | b |\n") {
        assert!(
            !rule.starts_with("table-column") && rule != "table-width-total",
            "{rule} fired on a paragraph"
        );
    }
    for rule in rules("text {widths=\"60,60\"}\n| a | b |\n") {
        assert!(rule != "table-width-total", "{rule} fired on a paragraph");
    }
}

#[test]
fn a_real_attribute_line_is_still_read() {
    // THE CONTROL for the gate above: every rule the ticket names keeps firing
    // when the line really is a block-attribute line.
    assert!(rules("{aligns=\"left\"}\n| a | b |\n").contains(&"table-column-arity"));
    assert!(rules("{widths=\"60,60\"}\n| a | b |\n").contains(&"table-width-total"));
    assert!(
        rules("{aligns=\"left,right\"}\n|=< a |=> b |\n").contains(&"table-column-overlap"),
        "the overlap rule still reads the table row"
    );
    // Indented, and with a trailing run after the closing brace, are both still
    // attribute lines.
    assert!(rules("  {aligns=\"left\"}\n  | a | b |\n").contains(&"table-column-arity"));
    assert!(rules("{aligns=\"left\"}  \n| a | b |\n").contains(&"table-column-arity"));
}

#[test]
fn a_table_column_diagnostic_names_a_codepoint_column() {
    // `{title="éé" aligns="left"}`: the `aligns` key starts at codepoint 13 and
    // byte 14. All three engines answer 13 on this document.
    let warnings = lint_carve("{title=\"éé\" aligns=\"left\"}\n| a | b |\n");
    let arity = warnings
        .iter()
        .find(|w| w.rule == "table-column-arity")
        .expect("the arity rule fired");
    assert_eq!(arity.column, 13);
    // `start` and `end` stay BYTES, which is what a Rust caller slices with.
    assert_eq!(arity.start, 14);

    // The alignment-run rule reads the same line the same way: `<` sits at
    // codepoint 7 of `| éé |<x |` and at byte 8.
    let padding = lint_carve("| éé |<x |\n");
    let run = padding
        .iter()
        .find(|w| w.rule == "table-alignment-run-padding")
        .expect("the padding rule fired");
    assert_eq!(run.column, 7);
    assert_eq!(run.start, 8);
}

fn first_block_span(source: &str) -> (usize, usize) {
    let options = Options {
        positions: true,
        ..Default::default()
    };
    let doc = parse_with_options(source, &options);
    let pos = match &doc.children[0] {
        BlockNode::Div(n) => n.pos.as_ref(),
        BlockNode::Admonition(n) => n.pos.as_ref(),
        BlockNode::LineBlock(n) => n.pos.as_ref(),
        BlockNode::BlockQuote(n) => n.pos.as_ref(),
        other => panic!("not a container: {other:?}"),
    }
    .expect("the block carries a position");
    (pos.start_line, pos.end_line)
}

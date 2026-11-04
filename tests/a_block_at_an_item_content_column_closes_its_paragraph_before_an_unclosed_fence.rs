//! An unclosed fence below a list in a container runs to the end of the
//! document under rule A (markup-carve/carve#3885), whether or not a block at
//! the item's content column closed the item's paragraph.

const RUNS_ON: &str =
    "more\n    </li>\n  </ul>\n  <pre><code>code\n::::\n:::\nafter *b*\n</code></pre>\n</aside>";
const RUNS_TO_END: &str = "  <pre><code>code\n::::\n:::\nafter *b*\n</code></pre>\n</aside>";

const CLOSING: &[(&str, &[&str])] = &[
    ("link definition", &["- para", "  [r]: /u"]),
    ("footnote definition", &["- para", "  [^f]: n"]),
    (
        "definition at the outer item column",
        &["- a", "  - para", "  [r]: /u"],
    ),
    ("table row", &["- para", "  | t |"]),
    ("ordered item table row", &["10. para", "    | t |"]),
    ("quoted item table row", &["> - para", ">   | t |"]),
    ("nested item table row", &["- a", "  - para", "    | t |"]),
    ("task item table row", &["- [ ] para", "  | t |"]),
    ("table row past the column", &["- para", "    | t |"]),
    ("attribute line", &["- para", "  {#x}"]),
    ("heading", &["- para", "  # H"]),
    (
        "table row under an attributed marker",
        &["-{#x} para", "  | t |"],
    ),
    (
        "table row under an attributed ordered marker",
        &["1.{#x} para", "   | t |"],
    ),
];

const OPEN: &[(&str, &[&str], &str)] = &[
    (
        "definition below the column",
        &["- para", " [r]: /u"],
        RUNS_TO_END,
    ),
    (
        "definition below the column of an item with trailing spaces",
        &["- para  ", " [r]: /u"],
        RUNS_TO_END,
    ),
    (
        "table row below the column",
        &["10. para", "   | t |"],
        RUNS_TO_END,
    ),
    (
        "item prose after a table row",
        &["- para", "  | t |", "  more"],
        RUNS_ON,
    ),
];

fn render(lines: &[&str]) -> String {
    let mut source = vec!["::: note"];
    source.extend_from_slice(lines);
    source.extend_from_slice(&["```", "code", "::::", ":::", "after *b*", ""]);
    carve::to_html(&source.join("\n"))
}

#[test]
fn a_block_at_an_item_content_column_leaves_the_fence_to_the_outer_closer_search() {
    let failures: Vec<_> = CLOSING
        .iter()
        .filter(|(_, lines)| !render(lines).trim_end_matches('\n').ends_with(RUNS_TO_END))
        .map(|(name, lines)| format!("{name}:\n{}", render(lines)))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

#[test]
fn a_paragraph_left_open_keeps_the_fence_as_text() {
    let failures: Vec<_> = OPEN
        .iter()
        .filter(|(_, lines, expected)| !render(lines).contains(&**expected))
        .map(|(name, lines, _)| format!("{name}:\n{}", render(lines)))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

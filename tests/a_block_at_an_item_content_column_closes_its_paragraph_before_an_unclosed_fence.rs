//! A block at an item's content column ends the item's paragraph
//! (CARVE-P0-010, CARVE-P0-011), so an unclosed fence below it opens no span
//! and the container's own closer ends it (A CLOSER IS REQUIRED).

const ENDS_AT_CLOSER: &str =
    "  <pre><code>code\n::::\n</code></pre>\n</aside>\n<p>after <strong>b</strong></p>";
const RUNS_ON: &str = "<p>after <strong>b</strong></p>\n    </div>";

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
        ENDS_AT_CLOSER,
    ),
    (
        "definition below the column of an item with trailing spaces",
        &["- para  ", " [r]: /u"],
        ENDS_AT_CLOSER,
    ),
    (
        "table row below the column",
        &["10. para", "   | t |"],
        ENDS_AT_CLOSER,
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
#[ignore = "rule A ledger: carve-rs#3525"]
fn a_block_at_an_item_content_column_closes_its_paragraph() {
    let failures: Vec<_> = CLOSING
        .iter()
        .filter(|(_, lines)| {
            !render(lines)
                .trim_end_matches('\n')
                .ends_with(ENDS_AT_CLOSER)
        })
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

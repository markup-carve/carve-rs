#[test]
fn a_line_block_keeps_the_smart_dash_without_padding_or_escaping() {
    let formatted = carve::to_carve("::: |\n---\n");
    assert_eq!(formatted, "::: |\n---\n:::\n");
    assert_eq!(
        carve::to_html(&formatted),
        "<div class=\"line-block\">\n  <p>—</p>\n</div>"
    );
}

#[test]
fn thematic_looking_lines_preserve_layout_and_settle() {
    let bodies = [
        "---",
        "----",
        "-----",
        "------",
        "-------",
        "--------",
        "---------",
        "---\nnext",
        "first\n---",
        "first\n---\nlast",
        "---\n\n---",
        " ---",
        "\t---",
        "---  ",
        "\\---",
        "---\\",
        "***",
        "___",
    ];
    for body in bodies {
        let block = format!("::: |\n{body}\n:::\n");
        let quoted = block
            .lines()
            .map(|line| format!("> {line}\n"))
            .collect::<String>();
        let listed = format!(
            "- item\n\n{}",
            block
                .lines()
                .map(|line| format!("  {line}\n"))
                .collect::<String>()
        );
        for source in [block, format!("::: |\n{body}\n"), quoted, listed] {
            let formatted = carve::to_carve(&source);
            assert_eq!(
                carve::to_html(&formatted),
                carve::to_html(&source),
                "{source:?}"
            );
            assert_eq!(carve::to_carve(&formatted), formatted, "{source:?}");
        }
    }
}

#[test]
fn ordinary_paragraph_guards_and_thematic_breaks_stay_unchanged() {
    for source in [" ---\n", "---\n"] {
        assert_eq!(carve::to_carve(source), source);
    }
}

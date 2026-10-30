//! `fmt` and the Djot importer keep the source's tree on the shapes from the
//! tickets this file's PR closes.

fn assert_fmt(source: &str, expected: &str) {
    let written = carve::to_carve(source);
    assert_eq!(written, expected);
    assert_eq!(carve::to_html(&written), carve::to_html(source), "{source}");
    assert_eq!(carve::to_carve(&written), written, "{source}");
}

#[test]
fn a_line_ending_inside_an_open_run_takes_no_padding_cell() {
    assert_fmt(
        "| a | b |\n+ c` |\n+ x | y |\n",
        "| a | b |\n+ c` |\n+ x | y` | |\n",
    );
    assert_fmt(
        "| a | b |\n+ c` | d |\n+ x | y |\n",
        "| a | b |\n+ c` | d |\n+ x | y` | |\n",
    );
    assert_fmt(
        "| a | b | c |\n+ x | y` |\n+ p | q | r |\n",
        "| a | b | c |\n+ x | y` |\n+ p | q | r` | |\n",
    );
    assert_fmt(
        "| a | b |\n+ ``c |\n+ x `` y | z |\n",
        "| a | b |\n+ `c |\n+ x ` y | z |\n",
    );
    assert_fmt(
        "| a | b |\n+ c | d` |\n+ x | y |\n",
        "| a | b |\n+ c | d` |\n+ x | y` |\n",
    );
}

#[test]
fn a_code_value_line_shaped_like_a_marker_stays_in_its_span() {
    assert_fmt("- `a\n . b`\n", "- `a\n . b`\n");
    assert_fmt("> - `a\n. b`\n", "> - `a\n>  . b`\n");
    assert_fmt("> - `a\n- b`\n", "> - `a\n- b`\n");
}

#[test]
fn a_djot_task_note_ends_before_the_text_below_it() {
    assert_eq!(
        carve::djot_to_carve("[^1]: + [x]\ntext\n\nt[^1]\n").trim_end(),
        "[^1]: - [x] {empty}\n\ntext\n\nt[^1]"
    );
    let fenced = "```\n[^1]: - [x] +\ntext\n```\n";
    assert_eq!(carve::djot_to_carve(fenced), fenced);
}

#[test]
fn a_row_attribute_value_keeps_its_pipe_unescaped_and_reads_back() {
    assert_eq!(
        carve::to_carve("| a | b |{a='x|y|z' .c}\n").trim_end(),
        "| a | b |{a=\"x|y|z\" .c}"
    );
    let values = [
        "|", "||", "x|", "|x", "x|y", r"\|", r"\\|", r"x\|y", "|}", "}|", "{|", "|{", "]|", "`|",
        "| |", "|=", "|{a}", "a b|c", "|'", r#"|\""#, "<|", "|>", "|.c", "|#d", "%|", "|%",
    ];
    let shells = [
        ("| a |", "\n"),
        ("|= a |", "\n"),
        ("| a | b |", "\n"),
        ("> | a |", "\n"),
        ("- | a |", "\n"),
        ("| h |\n| a |", "\n| z |\n"),
    ];
    for value in values {
        let quoted = value.replace('\\', r"\\").replace('"', r#"\""#);
        for (head, tail) in shells {
            let source = format!("{head}{{a=\"{quoted}\"}}{tail}");
            let html = carve::to_html(&source);
            let written = carve::to_carve(&source);
            assert_eq!(carve::to_html(&written), html, "{source:?} -> {written:?}");
            assert_eq!(carve::to_carve(&written), written, "{source:?}");
        }
    }
}

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

fn indent(source: &str, width: usize) -> String {
    source
        .lines()
        .map(|line| format!("{}{line}", " ".repeat(width)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn definition_and_footnote_bodies_accept_authored_block_bases() {
    let shapes = [
        "# h",
        "> q",
        "```\ncode\n```",
        "```=html\n<b>x</b>\n```",
        "%%%\nhidden\n%%%",
        "::: note\nbody\n:::",
        "| A |\n| b |",
        ":: term\n:  def",
        "- one\n  - two",
        "{.c}\n# h",
    ];

    for body in shapes {
        let exact_note = format!("[^n]: intro\n\n{}\n\nsee[^n]\n", indent(body, 2));
        let over_note = format!("[^n]: intro\n\n{}\n\nsee[^n]\n", indent(body, 3));
        assert_eq!(html(&over_note), html(&exact_note), "footnote: {body:?}");

        let exact_definition = format!(":: term\n:  intro\n\n{}\n", indent(body, 3));
        let over_definition = format!(":: term\n:  intro\n\n{}\n", indent(body, 4));
        assert_eq!(
            html(&over_definition),
            html(&exact_definition),
            "definition: {body:?}"
        );
    }
}

#[test]
fn a_link_definition_registers_at_an_authored_footnote_base() {
    let output = html("[^n]: note\n   [r]: /u\n\nsee[^n] and [t][r]\n");
    assert!(output.contains("<a href=\"/u\">t</a>"), "{output}");
    assert!(!output.contains("[r]: /u"), "{output}");
}

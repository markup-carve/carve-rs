fn html(source: &str) -> String {
    carve::render_html(&carve::parse(source))
        .unwrap()
        .lines()
        .map(str::trim)
        .collect::<String>()
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn table_after_invisible_definition_keeps_its_authored_base() {
    let table = "<table><tbody><tr><td>a</td></tr></tbody></table>";
    for (source, expected) in [
        ("- a\n  [^n]: note\n   q\n   | a |\n", format!("<ul><li>aq{table}</li></ul>")),
        ("- a\n  [n]: /u\n   q\n   | a |\n", format!("<ul><li>aq{table}</li></ul>")),
        ("- a\n  %% hidden\n   q\n   | a |\n", format!("<ul><li>aq{table}</li></ul>")),
        ("- a\n  %%%\n  hidden\n  %%%\n   q\n   | a |\n", format!("<ul><li>aq{table}</li></ul>")),
        ("- a\n  [^n]: note\n   q\n   r\n   | a |\n", format!("<ul><li>aqr{table}</li></ul>")),
        ("- a\n  [^n]: note\n   q\n  | a |\n", format!("<ul><li>aq{table}</li></ul>")),
        ("- a\n  [^n]: note\n   q\n    | a |\n", format!("<ul><li>aq{table}</li></ul>")),
        ("* [ ] task\n  [^n]: note\n   q\n   | a |\n", format!("<ul class=\"task-list\"><li><input type=\"checkbox\" disabled aria-label=\"task\"> taskq{table}</li></ul>")),
        ("- a\n  [^n]: note\n    * [ ] task\n   q\n   | a |\n", format!("<ul><li>aq{table}</li></ul>")),
        ("- a\n   q\n   | a |\n", format!("<ul><li>aq{table}</li></ul>")),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

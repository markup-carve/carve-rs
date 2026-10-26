//! Link destinations have no trailing whitespace. After a title, ASCII
//! spaces may pad the tail before `)`, while tabs remain invalid (carve#3671).
//! These rows cover inline links, inline images, and bare image lines.

/// Record a row that did not render as expected, so one run names every row
/// that moved instead of stopping at the first.
fn row(source: &str, expected: &str, moved: &mut Vec<String>) {
    let actual = carve::to_html(source);
    let actual = actual.trim_end();
    if actual != expected {
        moved.push(format!(
            "{source:?}\n  expected: {expected}\n  actual:   {actual}"
        ));
    }
}

#[test]
fn destination_runs_are_invalid_but_title_padding_is_allowed() {
    let mut moved = Vec::new();
    row("[t](a )\n", "<p>[t](a )</p>", &mut moved);
    row("[t](a  )\n", "<p>[t](a  )</p>", &mut moved);
    row("[t](a\t)\n", "<p>[t](a\t)</p>", &mut moved);
    row(
        "[t](/u \"T\" )\n",
        "<p><a href=\"/u\" title=\"T\">t</a></p>",
        &mut moved,
    );
    row("[t](/u \"T\"\t)\n", "<p>[t](/u “T”\t)</p>", &mut moved);
    row(
        "[t](/u 'T' )\n",
        "<p><a href=\"/u\" title=\"T\">t</a></p>",
        &mut moved,
    );
    row(
        "[t](/u \"T\" ){.c}\n",
        "<p><a href=\"/u\" title=\"T\" class=\"c\">t</a></p>",
        &mut moved,
    );
    row("a ![i](/i ) b\n", "<p>a ![i](/i ) b</p>", &mut moved);
    row("![a](/i )\n", "<p>![a](/i )</p>", &mut moved);
    row(
        "![a](/i \"T\" )\n",
        "<img src=\"/i\" alt=\"a\" title=\"T\">",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

/// CONTROLS: the same tails with nothing before the `)` still form, so the
/// check is pointed at the run and not at the construct.
#[test]
fn the_tail_without_the_run_still_forms() {
    let mut moved = Vec::new();
    row("[t](a)\n", "<p><a href=\"a\">t</a></p>", &mut moved);
    row(
        "[t](a \"T\")\n",
        "<p><a href=\"a\" title=\"T\">t</a></p>",
        &mut moved,
    );
    row(
        "[t](/u 'T'){.c}\n",
        "<p><a href=\"/u\" title=\"T\" class=\"c\">t</a></p>",
        &mut moved,
    );
    row(
        "a ![i](/i) b\n",
        "<p>a <img src=\"/i\" alt=\"i\"> b</p>",
        &mut moved,
    );
    row("![a](/i)\n", "<img src=\"/i\" alt=\"a\">", &mut moved);
    row("[t](a(b))\n", "<p><a href=\"a(b)\">t</a></p>", &mut moved);
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

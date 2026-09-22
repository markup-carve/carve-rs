//! CARVE-P3-013: a bare opener is not followed, and a bare closer not preceded,
//! by `ws`, and the grammar's guard class is `ws = space | tab | newline`. A tab
//! against a bare delimiter therefore refuses it exactly as a space does, for
//! all five bare delimiters and on both sides (carve-rs#1805).

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
fn a_tab_after_a_bare_opener_refuses_it() {
    let mut moved = Vec::new();
    row("a /\tx/ b\n", "<p>a /\tx/ b</p>", &mut moved);
    row("a *\tx* b\n", "<p>a *\tx* b</p>", &mut moved);
    row("a _\tx_ b\n", "<p>a _\tx_ b</p>", &mut moved);
    row("a ~\tx~ b\n", "<p>a ~\tx~ b</p>", &mut moved);
    row("a =\tx= b\n", "<p>a =\tx= b</p>", &mut moved);
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

#[test]
fn a_tab_before_a_bare_closer_refuses_it() {
    let mut moved = Vec::new();
    row("a /x\t/ b\n", "<p>a /x\t/ b</p>", &mut moved);
    row("a *x\t* b\n", "<p>a *x\t* b</p>", &mut moved);
    row("a _x\t_ b\n", "<p>a _x\t_ b</p>", &mut moved);
    row("a ~x\t~ b\n", "<p>a ~x\t~ b</p>", &mut moved);
    row("a =x\t= b\n", "<p>a =x\t= b</p>", &mut moved);
    // The refused closer is skipped, not the end of the search: a later one
    // still closes the span.
    row("a /x\t/y/ b\n", "<p>a <em>x\t/y</em> b</p>", &mut moved);
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

/// CONTROLS: the braced forms have no `ws` guard, a space was already refused,
/// and a bare pair with no whitespace against it still forms.
#[test]
fn the_neighbours_of_the_rule_are_unchanged() {
    let mut moved = Vec::new();
    row("a {/\tx/} b\n", "<p>a <em>\tx</em> b</p>", &mut moved);
    row(
        "a {*x\t*} b\n",
        "<p>a <strong>x\t</strong> b</p>",
        &mut moved,
    );
    row("a / x/ b\n", "<p>a / x/ b</p>", &mut moved);
    row("a /x / b\n", "<p>a /x / b</p>", &mut moved);
    row("a /x/ b\n", "<p>a <em>x</em> b</p>", &mut moved);
    row("a =x= b\n", "<p>a <mark>x</mark> b</p>", &mut moved);
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

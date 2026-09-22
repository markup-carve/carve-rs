//! `bold_italic = bare_opener('/'), '*', bi_content, '*', bare_closer('/'),
//! [attributes]` has the same trailing `[attributes]` slot as every other
//! inline carrier. The parser attached the block to the node, and the HTML
//! renderer then wrote the combined token without it, so the authored
//! characters reached neither the element nor the output (carve-rs#1795).

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
fn the_block_lands_on_the_outer_element() {
    let mut moved = Vec::new();
    row(
        "/*x*/{.k} y\n",
        "<p><strong class=\"k\"><em>x</em></strong> y</p>",
        &mut moved,
    );
    row(
        "/*x*/{#i} y\n",
        "<p><strong id=\"i\"><em>x</em></strong> y</p>",
        &mut moved,
    );
    row(
        "/*x*/{k=v} y\n",
        "<p><strong k=\"v\"><em>x</em></strong> y</p>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

/// CONTROLS: a combined token with no block, a block separated from it by a
/// space, and a block behind the word after the token all read as before.
#[test]
fn a_block_that_is_not_the_closers_is_unchanged() {
    let mut moved = Vec::new();
    row(
        "/*x*/ y\n",
        "<p><strong><em>x</em></strong> y</p>",
        &mut moved,
    );
    row(
        "/*x*/ {.k} y\n",
        "<p><strong><em>x</em></strong> {.k} y</p>",
        &mut moved,
    );
    row(
        "a/*y*/b{.k} c\n",
        "<p>a<strong><em>y</em></strong>b{.k} c</p>",
        &mut moved,
    );
    row(
        "*x*{.k} y\n",
        "<p><strong class=\"k\">x</strong> y</p>",
        &mut moved,
    );
    assert!(moved.is_empty(), "rows moved:\n{}", moved.join("\n"));
}

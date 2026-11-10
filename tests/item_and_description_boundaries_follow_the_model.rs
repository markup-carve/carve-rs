//! Item and description boundaries the spec model draws (carve-rs#3857, #3859, #3860).

fn assert_rows(rows: &[(&str, &str)]) {
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for (source, html) in rows {
        for output in [
            carve::to_html(source),
            carve::render_html(&carve::parse(source)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&output, "><").trim(),
                *html,
                "{source:?}"
            );
        }
    }
}

#[test]
fn a_line_after_a_sibling_ends_a_bare_line_block() {
    assert_rows(&[
        ("- a\n  - b\n\n    ::: |\n    text\n  - c\n      ```\n      > payload\n      ```\nafter\n", "<ul><li>a\n    <ul><li><p>b</p><div class=\"line-block\"><p>text</p></div></li><li><p>c</p><pre><code>&gt; payload\n</code></pre></li></ul></li></ul><p>after</p>"),
        ("- a\n  - b\n    ::: |\n    text\n  - c\n    # h\nafter\n", "<ul><li>a\n    <ul><li>b\n        <div class=\"line-block\"><p>text</p></div></li><li>c\n        <h1 id=\"h\">h</h1></li></ul></li></ul><p>after</p>"),
        ("- a\n  - b\n    ::: |\n    text\n  - c\n    | a |\nafter\n", "<ul><li>a\n    <ul><li>b\n        <div class=\"line-block\"><p>text</p></div></li><li>c\n        <table><tbody><tr><td>a</td></tr></tbody></table></li></ul></li></ul><p>after</p>"),
    ]);
}

#[test]
fn a_blank_before_a_refused_plus_stays_in_the_deeper_item() {
    assert_rows(&[
        ("- - - ```\n\n    +\n  x\n", "<ul><li><ul><li><ul><li><pre><code></code></pre></li></ul></li></ul>\n    x\n  </li></ul>"),
        ("- a\n  - b\n    - ```\n\n    +\n  x\n", "<ul><li>a\n    <ul><li>b\n        <ul><li><pre><code></code></pre></li></ul></li></ul>\n    x\n  </li></ul>"),
        ("- a\n  - b\n    - ```\n     +\ntail\n", "<ul><li>a\n    <ul><li>b\n        <ul><li><pre><code></code></pre></li></ul>\n        +\ntail\n      </li></ul></li></ul>"),
        ("- a0\n  - ```\n\n    +\n  x\n", "<ul><li>a0\n    <ul><li><pre><code>\n+\n</code></pre></li></ul>\n    x\n  </li></ul>"),
    ]);
}

#[test]
#[ignore = "carve-rs#4184"]
fn a_refused_plus_after_a_blank_keeps_the_follower_in_the_deeper_item() {
    // The owner follows carve#5637; the looseness still differs from the model.
    assert_rows(&[(
        "- a0\n  - ```\n\n  +\n    x\n",
        "<ul><li><p>a0</p><ul><li><pre><code></code></pre>\n        x\n      </li></ul></li></ul>",
    )]);
}

#[test]
fn a_description_marker_below_the_column_ends_a_plus_paragraph() {
    assert_rows(&[
        (":: term\n: body\n+\n`x``\n : description`\n", "<dl><dt>term</dt><dd><p>body</p><p><code>x``</code></p></dd><dd>description<code></code></dd></dl>"),
        (":: term\n: body\n+\nx\n : description\n", "<dl><dt>term</dt><dd><p>body</p><p>x</p></dd><dd>description</dd></dl>"),
        (":: term\n: body\n+\nx\n  : description\n", "<dl><dt>term</dt><dd><p>body</p><p>x\n: description</p></dd></dl>"),
    ]);
}

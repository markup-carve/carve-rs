//! A quote's `+` block ends at its boundary: a code fence it leaves open behind
//! a list or description marker gets no payload line the source does not hold,
//! and a line the block did not take ends the fence and the quote (PART 9 §17
//! L3, markup-carve/carve#5045). Expected HTML is the spec model's at carve `5f8be22b`.

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_nested_fence_left_open_takes_no_blank_payload() {
    let cases = [
        (
            "> :: t\n+\n: ``` js\n> q\n",
            "<blockquote><dl><dt>t</dt><dd><pre><code class=\"language-js\"></code></pre></dd></dl><p>q</p></blockquote>",
        ),
        (
            "> a\n+\n- a\n  ``` js\n> q\n",
            "<blockquote><p>a</p><ul><li>a<pre><code class=\"language-js\"></code></pre></li></ul><p>q</p></blockquote>",
        ),
        (
            "> a\n+\n: ``` js\n> q\n",
            "<blockquote><p>a</p><p>: <code> js</code></p><p>q</p></blockquote>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn an_unmarked_line_after_the_open_fence_ends_the_quote() {
    assert_eq!(
        flat("> - x\n+\n:: t\n: ``` js\nq\n"),
        "<blockquote><ul><li>x</li></ul><dl><dt>t</dt><dd><pre><code class=\"language-js\"></code></pre></dd></dl></blockquote><p>q</p>"
    );
}

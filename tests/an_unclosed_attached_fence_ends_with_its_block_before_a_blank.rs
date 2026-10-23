//! An unclosed fence a first-block `+` attaches takes the rest of its container
//! only when no blank line follows; otherwise it ends with the attached block,
//! as the spec model and carve-js read it (markup-carve/carve#3421).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn the_unclosed_fence_reads_like_the_references() {
    for (source, expected) in [
        (
            "- +\n```\n- x\n",
            "<ul><li><pre><code>-x</code></pre></li></ul>",
        ),
        (
            "- +\n```\n- x\n- y\n",
            "<ul><li><pre><code>-x-y</code></pre></li></ul>",
        ),
        (
            "- +\n```\nx\n- y\n\npara\n",
            "<ul><li><pre><code>x</code></pre></li><li>y</li></ul><p>para</p>",
        ),
        (
            "- +\n```\nx\n\ny\n",
            "<ul><li><pre><code>x</code></pre></li></ul><p>y</p>",
        ),
        (
            "- +\n```\nx\n\n```\n",
            "<ul><li><pre><code>x</code></pre></li></ul>",
        ),
        (
            "- +\n```\nx\n~~~\n- y\n\n~~~\n",
            "<ul><li><pre><code>x~~~</code></pre></li><li>y</li></ul><pre><code></code></pre>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

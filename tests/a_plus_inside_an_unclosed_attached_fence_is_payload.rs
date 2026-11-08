// carve#5494: a lone `+` inside an unclosed fence a `+` attached is fence
// content, whether or not a blank line follows (PART 9 §17 L3).

fn html(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn the_plus_is_code_before_a_later_blank() {
    for fence in ["```", "~~~", "````"] {
        let cases = [
            (
                format!("- +\n{fence}\n+\n\ny\n"),
                "<ul><li><pre><code>+</code></pre></li></ul><p>y</p>",
            ),
            (
                format!("1. +\n{fence}\n+\n\ny\n"),
                "<ol><li><pre><code>+</code></pre></li></ol><p>y</p>",
            ),
            (
                format!("> +\n{fence}\n+\n\ny\n"),
                "<blockquote><pre><code>+</code></pre></blockquote><p>y</p>",
            ),
            (
                format!("- - +\n  {fence}\n  +\n\ny\n"),
                "<ul><li><ul><li><pre><code>+</code></pre></li></ul></li></ul><p>y</p>",
            ),
            (
                format!("- +\n{fence}\nx\n+\n\ny\n"),
                "<ul><li><pre><code>x+</code></pre></li></ul><p>y</p>",
            ),
            (
                format!("- +\n{fence}\n+\n+\n\ny\n"),
                "<ul><li><pre><code>++</code></pre></li></ul><p>y</p>",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(html(&source), expected, "{source}");
        }
    }
}

#[test]
fn the_plus_is_code_without_a_later_blank() {
    assert_eq!(
        html("- +\n```\n+\n"),
        "<ul><li><pre><code>+</code></pre></li></ul>"
    );
}

#[test]
fn a_note_plus_inside_its_unclosed_attached_fence_is_code() {
    let out = html("[^1]: +\n```\n+\nb\n\nsee[^1]\n");
    assert!(
        out.contains("<li id=\"fn1\"><pre><code>+b</code></pre>"),
        "{out}"
    );
    assert!(!out.contains("<p>+"), "{out}");
}

#[test]
fn a_plus_after_the_closed_attached_fence_still_reaches_the_item() {
    assert_eq!(
        html("- +\n```\nq\n```\n+\nz\n"),
        "<ul><li><pre><code>q</code></pre>z</li></ul>"
    );
}

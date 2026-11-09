// markup-carve/carve#5632: inline code pairing hides a brace run, so a later
// interrupting fence remains a block opener.
use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_brace_line_inside_open_code_does_not_protect_the_fence() {
    let source = "p\n foo](`url)\n {.x\n .y}\n ```\n z\n";
    let expected = "<p>p\nfoo](<code>url)\n{.x\n.y}</code></p>\n<pre><code>z\n</code></pre>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_tilde_fence_after_an_open_code_span_also_interrupts() {
    let source = "p\n `open\n {.x\n .y}\n ~~~\n z\n";
    let expected = "<p>p\n<code>open\n{.x\n.y}</code></p>\n<pre><code>z\n</code></pre>";
    assert_eq!(to_html(source), expected);
}

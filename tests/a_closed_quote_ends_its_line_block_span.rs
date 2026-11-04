//! A lazy line ends the quote, so a line block's open backtick or tilde span
//! cannot reach a later quote's fence opener (carve#5062, CARVE-P0-024).

use carve::to_html;

#[test]
fn a_later_quote_opens_its_own_fence() {
    let html = to_html("> - ::: |\n>   ```\n>   :::\nlazy\n>   ```\n>   x\n");
    assert_eq!(
        html,
        "<blockquote>\n  <ul>\n    <li>\n      <div class=\"line-block\">\n        <p><code></code></p>\n      </div>\n    </li>\n  </ul>\n</blockquote>\n<p>lazy</p>\n<blockquote>\n  <pre><code>x\n</code></pre>\n</blockquote>"
    );
}

#[test]
fn every_quote_host_and_run_ends_the_span() {
    for (open, cont) in [
        ("> - ", ">   "),
        ("> ", "> "),
        ("> > - ", "> >   "),
        ("> > ", "> > "),
    ] {
        for run in ["```", "~~~", "````"] {
            let source = format!("{open}::: |\n{cont}{run}\n{cont}:::\nlazy\n>   {run}\n>   x\n");
            let html = to_html(&source);
            assert!(
                html.contains("<div class=\"line-block\">"),
                "{source}\n{html}"
            );
            assert!(
                html.ends_with(
                    "<p>lazy</p>\n<blockquote>\n  <pre><code>x\n</code></pre>\n</blockquote>"
                ),
                "{source}\n{html}"
            );
        }
    }
}

#[test]
fn a_comment_span_still_ends_at_the_lazy_line() {
    let source = "> ::: |\n> %%%\n> :::\nlazy\n> ```\n> x\n";
    let html = to_html(source);
    assert!(
        html.ends_with("<blockquote>\n  <pre><code>x\n</code></pre>\n</blockquote>"),
        "{html}"
    );
}

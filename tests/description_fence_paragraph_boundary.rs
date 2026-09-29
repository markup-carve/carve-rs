// HTML expectations measured with the executable spec at 38829a97.
use carve::{to_html, to_html_with_options, Options};

#[test]
fn c2092_dd_at_column() {
    let source = ":: t\n:  head\n\n   ```\n   a\n```\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <pre><code>a\n</code></pre>\n  </dd>\n</dl>\n<pre><code></code></pre>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2092_dd_code_unterm_below_col() {
    let source = ":: t\n:  head\n\n   ```\n   a\n  ```\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <pre><code>a\n</code></pre>\n  </dd>\n</dl>\n<p><code></code></p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2092_dd_no_blank() {
    let source = ":: t\n:  head\n   ```\n   a\n```\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>head\n<code>\na\n</code></dd>\n</dl>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2092_reach_dd_code_base3_close0() {
    let source = ":: t\n:  head\n\n   ```\n   a\n```\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <pre><code>a\n</code></pre>\n  </dd>\n</dl>\n<pre><code></code></pre>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2092_reach_dd_code_base3_close1() {
    let source = ":: t\n:  head\n\n   ```\n   a\n ```\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <pre><code>a\n</code></pre>\n  </dd>\n</dl>\n<p><code></code></p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2092_reach_dd_code_base3_close2() {
    let source = ":: t\n:  head\n\n   ```\n   a\n  ```\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <pre><code>a\n</code></pre>\n  </dd>\n</dl>\n<p><code></code></p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_2092_colon_spelling() {
    let source = ":: t\n:  head\n\n   ::: note\n   a\n:::\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a</p>\n    </aside>\n  </dd>\n</dl>\n<div>\n\n</div>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_2092_over_indented() {
    let source = ":: t\n:  head\n\n    ```\n    a\n    ```\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <pre><code>a\n</code></pre>\n  </dd>\n</dl>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

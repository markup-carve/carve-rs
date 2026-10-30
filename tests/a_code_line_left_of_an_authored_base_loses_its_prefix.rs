//! A code line written left of an over-indented fence opener is measured from
//! the fence's authored base and loses its whole available prefix
//! (CARVE-P0-004).

fn compact(source: &str) -> String {
    carve::to_html(source)
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn a_definition_body_line_left_of_the_opener_loses_its_prefix() {
    assert_eq!(
        compact(":: t\n: x\n\n     ```\n    a\n     b\n     ```\n"),
        "<dl>|<dt>t</dt>|<dd>|<p>x</p>|<pre><code>a|b|</code></pre>|</dd>|</dl>"
    );
}

#[test]
fn a_list_item_line_left_of_the_opener_loses_its_prefix() {
    assert_eq!(
        compact("- x\n\n     ```\n    a\n     b\n     ```\n"),
        "<ul>|<li>x|<pre><code>a|b|</code></pre>|</li>|</ul>"
    );
}

#[test]
fn a_matching_run_left_of_the_opener_closes_after_losing_its_prefix() {
    assert_eq!(
        compact("- x\n\n     ```\n   ```\n     b\n     ```\n"),
        "<ul>|<li>x|<pre><code></code></pre>|b|<pre><code></code></pre>|</li>|</ul>"
    );
}

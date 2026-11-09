use carve::to_html;

#[test]
fn quoted_verse_span_does_not_pair_past_quote_exit() {
    let source = "> - ::: |\n>   ```\nz\n>   ```\n";
    let expected = "<blockquote>\n  <ul>\n    <li>\n      <div class=\"line-block\">\n        <p><code>\nz\n</code></p>\n      </div>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(to_html(source).trim(), expected);
}

#[test]
fn closed_sibling_fence_in_nested_quote_does_not_hold_outer_quote() {
    let source = "> > 1. ::: |\n> >   ```\nz\n";
    let expected = "<blockquote>\n  <blockquote>\n    <ol>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ol>\n    <pre><code></code></pre>\n  </blockquote>\n</blockquote>\n<p>z</p>";
    assert_eq!(to_html(source).trim(), expected);
}

#[test]
fn postblank_admonition() {
    assert_eq!(
        to_html(
            r####"- a

  x
   ::: note
"####
        )
        .trim(),
        r####"<ul>
  <li><p>a</p>
    <p>x</p>
    <aside class="admonition note" aria-label="Note">

    </aside>
  </li>
</ul>"####
            .trim()
    );
}

#[test]
fn postblank_heading_and_sibling() {
    assert_eq!(
        to_html(
            r####"- a

  x
   # H
tail
"####
        )
        .trim(),
        r####"<ul>
  <li><p>a</p>
    <p>x</p>
    <h1 id="H">H</h1>
  </li>
</ul>
<p>tail</p>"####
            .trim()
    );
}

#[test]
fn ordered_below_column_follower() {
    assert_eq!(
        to_html(
            r####"1. a

   x
    # H
  z
"####
        )
        .trim(),
        r####"<ol>
  <li><p>a</p>
    <p>x</p>
    <h1 id="H">H</h1>
  </li>
</ol>
<p>z</p>"####
            .trim()
    );
}

#[test]
fn shifted_definition_heading() {
    assert_eq!(
        to_html(
            r####"- * :: t
  : a
    # H
    tail
"####
        )
        .trim(),
        r####"<ul>
  <li>
    <ul>
      <li>
        <dl>
          <dt>t</dt>
        </dl>
      </li>
    </ul>
    : a
    <h1 id="H">H</h1>
    tail
  </li>
</ul>"####
            .trim()
    );
}

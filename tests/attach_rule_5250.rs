use carve::to_html;

#[test]
fn attached_blocks_follow_one_owner_rule() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "855-a-plus-after-a-table-with-an-indented-row-attaches",
            r########"- a
+
|x|
 |y|
+
H
"########,
            r########"<ul>
  <li>a
    <table>
      <tbody>
        <tr><td>x</td></tr>
        <tr><td>y</td></tr>
      </tbody>
    </table>
    H
  </li>
</ul>
"########,
        ),
        (
            "855-a-refused-plus-after-an-attached-block-ends-nothing-2",
            r########"- a
  - b
  +
  ```
```
+
 ```
"########,
            r########"<ul>
  <li>a
    <ul>
      <li>b
        <pre><code></code></pre>
      </li>
    </ul>
  </li>
</ul>
<pre><code>+
 ```
</code></pre>
"########,
        ),
        (
            "855-a-refused-plus-after-an-attached-block-ends-nothing",
            r########"- o
+
> q
  +
  > r
"########,
            r########"<ul>
  <li>o
    <blockquote><p>q
+
r</p></blockquote>
  </li>
</ul>
"########,
        ),
        (
            "873-a-plus-after-an-attached-block-reaches-its-host-through-quote-prefixes-2",
            r########"> - a
> +
> # h
>
> +
> # h
"########,
            r########"<blockquote>
  <ul>
    <li>a
      <h1 id="h">h</h1>
      <h1 id="h-2">h</h1>
    </li>
  </ul>
</blockquote>
"########,
        ),
        (
            "880-an-own-line-plus-reaches-a-marker-line-quote-at-its-authored-column-3",
            r########"[^f]: > > a
        +
      > b

[^f]
"########,
            r########"<p><a id="fnref1" href="#fn1" role="doc-noteref"><sup>1</sup></a></p>
<section role="doc-endnotes" aria-label="Footnotes">
  <hr>
  <ol>
    <li id="fn1">
      <blockquote>
        <blockquote>
          <p>a</p>
          <p>b</p>
        </blockquote>
      </blockquote>
      <p><a href="#fnref1" role="doc-backlink" aria-label="Back to reference">↩</a></p>
    </li>
  </ol>
</section>
"########,
        ),
        (
            "948-a-blank-inside-a-attached-description-keeps-the-list-tight-3",
            r########"- a
  - b
  +
  :: t
  : d

    x
"########,
            r########"<ul>
  <li>a
    <ul>
      <li>b
        <dl>
          <dt>t</dt>
          <dd>
            <p>d</p>
            <p>x</p>
          </dd>
        </dl>
      </li>
    </ul>
  </li>
</ul>
"########,
        ),
        (
            "948-a-fence-a-quote-s-attached-block-opens-ends-with-its-container-3",
            r########"> +
- ``` js
- x
> q
"########,
            r########"<blockquote>
  <ul>
    <li>
      <pre><code class="language-js"></code></pre>
    </li>
    <li>x</li>
  </ul>
  <p>q</p>
</blockquote>
"########,
        ),
        (
            "948-a-fence-a-quote-s-attached-block-opens-ends-with-its-container-4",
            r########"> a
+
- ``` js
- x
> q
"########,
            r########"<blockquote>
  <p>a</p>
  <ul>
    <li>
      <pre><code class="language-js"></code></pre>
    </li>
    <li>x</li>
  </ul>
  <p>q</p>
</blockquote>
"########,
        ),
    ];
    for &(name, source, expected) in cases {
        assert_eq!(to_html(source).trim_end(), expected.trim_end(), "{name}");
    }
}

#[test]
fn quoted_container_fences_match_corpus_514() {
    let cases = [
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-10"####,
            r####"> - a
>
>   ```
>   x
> after
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
      <pre><code>x
</code></pre>
    </li>
  </ul>
  <p>after
flush</p>
</blockquote>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-11"####,
            r####"> :  a
>
>    ```
>    x
flush
"####,
            r####"<blockquote>
  <p>:  a</p>
  <p><code>
x
flush</code></p>
</blockquote>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-12"####,
            r####"> :: t
> :  d
>    - ```
>      x
flush
"####,
            r####"<blockquote>
  <dl>
    <dt>t</dt>
    <dd>d
- <code>
x
flush</code></dd>
  </dl>
</blockquote>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-2"####,
            r####"> - a
>   - b
>
>     ```
>     x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
      <ul>
        <li>b
          <pre><code>x
</code></pre>
        </li>
      </ul>
    </li>
  </ul>
</blockquote>
<p>flush</p>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-3"####,
            r####"> - a
>
>   ```
>   x
>   ```
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
      <pre><code>x
</code></pre>
    </li>
  </ul>
</blockquote>
<p>flush</p>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-4"####,
            r####"> - a
>   - b
>
>     ```
>     x
>     ```
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
      <ul>
        <li>b
          <pre><code>x
</code></pre>
        </li>
      </ul>
    </li>
  </ul>
</blockquote>
<p>flush</p>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-5"####,
            r####"> - ```
>   x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>
      <pre><code>x
</code></pre>
    </li>
  </ul>
</blockquote>
<p>flush</p>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-6"####,
            r####"> [^f]: t
>
>   ```
>   x
flush
"####,
            r####"<blockquote>

</blockquote>
<p>flush</p>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-7"####,
            r####"> - a
>
>   ```
>   x
>   ```
>
>   z
flush
"####,
            r####"<blockquote>
  <ul>
    <li><p>a</p>
      <pre><code>x
</code></pre>
      <p>z
flush</p>
    </li>
  </ul>
</blockquote>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-8"####,
            r####"> > - a
> >
> >   ```
> >   x
flush
"####,
            r####"<blockquote>
  <blockquote>
    <ul>
      <li>a
        <pre><code>x
</code></pre>
      </li>
    </ul>
  </blockquote>
</blockquote>
<p>flush</p>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-9"####,
            r####"> - a
> - ```
>   x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a</li>
    <li>
      <pre><code>x
</code></pre>
    </li>
  </ul>
</blockquote>
<p>flush</p>
"####,
        ),
        (
            r####"514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim"####,
            r####"> - a
>
>   ```
>   x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
      <pre><code>x
</code></pre>
    </li>
  </ul>
</blockquote>
<p>flush</p>
"####,
        ),
        (
            r####"a lazy line preserves the item column"####,
            r####"> - a
b
>
>   ```
>   x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
b
      <pre><code>x
</code></pre>
    </li>
  </ul>
</blockquote>
<p>flush</p>"####,
        ),
        (
            r####"a lazy line before a closed fence"####,
            r####"> - a
b
>
>   ```
>   x
>   ```
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
b
      <pre><code>x
</code></pre>
    </li>
  </ul>
</blockquote>
<p>flush</p>"####,
        ),
        (
            r####"a lazy line preserves nested item columns"####,
            r####"> - a
>   - b
c
>
>     ```
>     x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
      <ul>
        <li>b
c
          <pre><code>x
</code></pre>
        </li>
      </ul>
    </li>
  </ul>
</blockquote>
<p>flush</p>"####,
        ),
        (
            r####"an invalid info string leaves no closer"####,
            r####"> - a
>   ```bad`
>
>   ```
>   x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
<code>bad`</code>
      <pre><code>x
</code></pre>
    </li>
  </ul>
</blockquote>
<p>flush</p>"####,
        ),
        (
            r####"over-indented runs pair"####,
            r####"> - a
>     ~~~
>     x
>     ~~~
>
>   ~~~
>   x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
      <pre><code>x
</code></pre>
      <pre><code>x
</code></pre>
    </li>
  </ul>
</blockquote>
<p>flush</p>"####,
        ),
        (
            r####"an over-indented opener consumes its closer"####,
            r####"> - a
>     ```
>
>   ```
>   a
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
      <pre><code>
</code></pre>
      a
flush
    </li>
  </ul>
</blockquote>"####,
        ),
        (
            r####"a fence without a closer stays inline"####,
            r####"> - a
>   ```
>   x
flush
"####,
            r####"<blockquote>
  <ul>
    <li>a
<code>
x
flush</code></li>
  </ul>
</blockquote>"####,
        ),
    ];
    for (name, source, expected) in cases {
        let expected = expected.trim_end();
        assert_eq!(carve::to_html(source), expected, "{name}");
        let json = carve::to_json(&carve::parse(source));
        let decoded = carve::from_json(&json).expect("the parser's AST must ingest");
        assert_eq!(carve::render_html(&decoded).unwrap(), expected, "{name}");
    }
}

#[test]
fn code_and_raw_fences_across_quote_depths_and_marker_widths() {
    for depth in [1, 2, 3] {
        for fence in ["```", "~~~", "```=html"] {
            for (marker, column) in [("- ", 2), ("1. ", 3), ("- [x] ", 2), ("-{.x} ", 2)] {
                for closed in [false, true] {
                    let quote = "> ".repeat(depth);
                    let pad = " ".repeat(column);
                    let closer = if fence.starts_with('~') { "~~~" } else { "```" };
                    let ending = if closed {
                        format!("{quote}{pad}{closer}\n")
                    } else {
                        String::new()
                    };
                    let source = format!(
                        "{quote}{marker}a\n{}\n{quote}{pad}{fence}\n{quote}{pad}x\n{ending}flush\n",
                        quote.trim_end()
                    );
                    assert!(
                        carve::to_html(&source).ends_with("</blockquote>\n<p>flush</p>"),
                        "{source}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_nested_quote_ends_the_previous_item_host() {
    let source = "> - a\n> > q\n>\n>   ```\n>   x\nflush\n";
    assert!(carve::to_html(source).ends_with("<p><code>\nx\nflush</code></p>\n</blockquote>"));
}

#[test]
fn a_lazy_code_span_has_no_internal_frame_or_out_of_source_position() {
    let source = "> :: t\n> :  d\n>    - ```\n>      x\nflush\n";
    let doc = carve::parse_with_options(source, &carve::Options::default().with_positions(true));
    let json = carve::to_json(&doc);
    assert!(!json.contains("\\u0000"));
    fn check(value: &serde_json::Value, length: usize) {
        match value {
            serde_json::Value::Object(fields) => {
                if let Some(end) = fields.get("endOffset").and_then(|v| v.as_u64()) {
                    assert!(end <= length as u64, "{value}");
                }
                for child in fields.values() {
                    check(child, length);
                }
            }
            serde_json::Value::Array(items) => {
                for child in items {
                    check(child, length);
                }
            }
            _ => {}
        }
    }
    check(&serde_json::from_str(&json).unwrap(), source.len());
}

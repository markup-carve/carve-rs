// HTML expectations measured with the executable spec at 38829a97.
use carve::{to_html, to_html_with_options, Options};

#[test]
fn c2477_dd_col3_base5_close3() {
    let source = ":: t\n:  head\n\n     ::: note\n     a\n   :::\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a</p>\n    </aside>\n  </dd>\n</dl>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_dd_col3_base5_close4() {
    let source = ":: t\n:  head\n\n     ::: note\n     a\n    :::\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a\n:::</p>\n    </aside>\n  </dd>\n</dl>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_dd_col3_base5_close5() {
    let source = ":: t\n:  head\n\n     ::: note\n     a\n     :::\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a</p>\n    </aside>\n  </dd>\n</dl>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_dd_col3_base5_close6() {
    let source = ":: t\n:  head\n\n     ::: note\n     a\n      :::\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>head</p>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a\n:::</p>\n    </aside>\n  </dd>\n</dl>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_footnote_col6_base8_close6() {
    let source = "[^a]: x\n\n        ::: note\n        a\n      :::\n\ntext[^a]\n";
    let expected = "<p>text<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x</p>\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a\n:::</p>\n      </aside>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_footnote_col6_base8_close7() {
    let source = "[^a]: x\n\n        ::: note\n        a\n       :::\n\ntext[^a]\n";
    let expected = "<p>text<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x</p>\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a\n:::</p>\n      </aside>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_footnote_col6_base8_close8() {
    let source = "[^a]: x\n\n        ::: note\n        a\n        :::\n\ntext[^a]\n";
    let expected = "<p>text<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x</p>\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a</p>\n      </aside>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_footnote_col6_base8_close9() {
    let source = "[^a]: x\n\n        ::: note\n        a\n         :::\n\ntext[^a]\n";
    let expected = "<p>text<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x</p>\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a\n:::</p>\n      </aside>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_list_col2_base4_close2() {
    let source = "- x\n\n    ::: note\n    a\n  :::\n";
    let expected = "<ul>\n  <li>x\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a</p>\n    </aside>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_list_col2_base4_close3() {
    let source = "- x\n\n    ::: note\n    a\n   :::\n";
    let expected = "<ul>\n  <li>x\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a\n:::</p>\n    </aside>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_list_col2_base4_close4() {
    let source = "- x\n\n    ::: note\n    a\n    :::\n";
    let expected = "<ul>\n  <li>x\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a</p>\n    </aside>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_list_col2_base4_close5() {
    let source = "- x\n\n    ::: note\n    a\n     :::\n";
    let expected = "<ul>\n  <li>x\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a\n:::</p>\n    </aside>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_nested_col4_base6_close4() {
    let source = "- - x\n\n      ::: note\n      a\n    :::\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>x\n        <aside class=\"admonition note\" aria-label=\"Note\">\n          <p>a</p>\n        </aside>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_nested_col4_base6_close5() {
    let source = "- - x\n\n      ::: note\n      a\n     :::\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>x\n        <aside class=\"admonition note\" aria-label=\"Note\">\n          <p>a\n:::</p>\n        </aside>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_nested_col4_base6_close6() {
    let source = "- - x\n\n      ::: note\n      a\n      :::\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>x\n        <aside class=\"admonition note\" aria-label=\"Note\">\n          <p>a</p>\n        </aside>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_nested_col4_base6_close7() {
    let source = "- - x\n\n      ::: note\n      a\n       :::\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>x\n        <aside class=\"admonition note\" aria-label=\"Note\">\n          <p>a\n:::</p>\n        </aside>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_quoteitem_col4_base6_close4() {
    let source = "> - x\n>\n>     ::: note\n>     a\n>   :::\n";
    let expected = "<blockquote>\n  <ul>\n    <li>x\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a</p>\n      </aside>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_quoteitem_col4_base6_close5() {
    let source = "> - x\n>\n>     ::: note\n>     a\n>    :::\n";
    let expected = "<blockquote>\n  <ul>\n    <li>x\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a\n:::</p>\n      </aside>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_quoteitem_col4_base6_close6() {
    let source = "> - x\n>\n>     ::: note\n>     a\n>     :::\n";
    let expected = "<blockquote>\n  <ul>\n    <li>x\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a</p>\n      </aside>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn c2477_quoteitem_col4_base6_close7() {
    let source = "> - x\n>\n>     ::: note\n>     a\n>      :::\n";
    let expected = "<blockquote>\n  <ul>\n    <li>x\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a\n:::</p>\n      </aside>\n    </li>\n  </ul>\n</blockquote>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_codefence_close2() {
    let source = "- x\n\n    ```\n    a\n  ```\n\nafter\n";
    let expected = "<ul>\n  <li>x\n    <pre><code>a\n</code></pre>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_codefence_close3() {
    let source = "- x\n\n    ```\n    a\n   ```\n\nafter\n";
    let expected =
        "<ul>\n  <li>x\n    <pre><code>a\n ```\n\n</code></pre>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_codefence_close4() {
    let source = "- x\n\n    ```\n    a\n    ```\n\nafter\n";
    let expected = "<ul>\n  <li>x\n    <pre><code>a\n</code></pre>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_codefence_close5() {
    let source = "- x\n\n    ```\n    a\n     ```\n\nafter\n";
    let expected =
        "<ul>\n  <li>x\n    <pre><code>a\n ```\n\n</code></pre>\n  </li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_commentfence_close2() {
    let source = "- x\n\n    %%%\n    a\n  %%%\n\nafter\n";
    let expected = "<ul>\n  <li>x</li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_commentfence_close3() {
    let source = "- x\n\n    %%%\n    a\n   %%%\n\nafter\n";
    let expected = "<ul>\n  <li>x</li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_commentfence_close4() {
    let source = "- x\n\n    %%%\n    a\n    %%%\n\nafter\n";
    let expected = "<ul>\n  <li>x</li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn guard_commentfence_close5() {
    let source = "- x\n\n    %%%\n    a\n     %%%\n\nafter\n";
    let expected = "<ul>\n  <li>x</li>\n</ul>\n<p>after</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn footnote_closer_at_host_column_two() {
    let source = "[^a]: x\n\n        ::: note\n        a\n  :::\n\ntext[^a]\n";
    let expected = "<p>text<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>x</p>\n      <aside class=\"admonition note\" aria-label=\"Note\">\n        <p>a</p>\n      </aside>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

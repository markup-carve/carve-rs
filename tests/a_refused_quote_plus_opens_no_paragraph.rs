//! A refused first-block `+` in a quote opens no paragraph (PART 9 §17 L3), so a
//! column-0 line after it leaves the item and the line below opens its own block
//! (markup-carve/carve#5156).

use carve::to_html;

const EMPTY_ITEM_QUOTE: &str = "<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n";

#[test]
fn a_quote_after_the_document_paragraph_opens() {
    assert_eq!(
        to_html("- > +\np\n  > q\n"),
        format!("{EMPTY_ITEM_QUOTE}<p>p</p>\n<blockquote><p>q</p></blockquote>")
    );
}

#[test]
fn other_openers_after_the_document_paragraph_interrupt_it() {
    assert_eq!(
        to_html("- > +\np\n  ***\n"),
        format!("{EMPTY_ITEM_QUOTE}<p>p</p>\n<hr>")
    );
    assert_eq!(
        to_html("- > +\np\n  ```\n  x\n  ```\n"),
        format!("{EMPTY_ITEM_QUOTE}<p>p</p>\n<pre><code>x\n</code></pre>")
    );
}

#[test]
fn a_nested_quote_and_a_quote_below_item_text_read_the_same() {
    let html = to_html("- > > +\np\n  > q\n");
    assert!(html.ends_with("</ul>\n<p>p</p>\n<blockquote><p>q</p></blockquote>"), "{html}");
    let html = to_html("- a\n  > +\np\n  > q\n");
    assert!(html.ends_with("</ul>\n<p>p</p>\n<blockquote><p>q</p></blockquote>"), "{html}");
}

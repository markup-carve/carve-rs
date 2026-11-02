//! PART 2 `line_block`: a payload line is inline content whatever it looks
//! like, so the stanza stays open for a lazy line behind any list and quote
//! hosts (carve#4748).

use carve::to_html;

#[test]
fn a_hosted_stanza_takes_the_lazy_line() {
    for (source, stanza) in [
        ("- > ::: |\n  > %% c\nb\n", "<p><br>\nb</p>"),
        ("- > ::: |\n  > %% c\n  b\n", "<p><br>\nb</p>"),
        ("- - > ::: |\n    > %% c\nb\n", "<p><br>\nb</p>"),
        ("- x\n\n  - > ::: |\n    > %% c\nb\n", "<p><br>\nb</p>"),
        ("- x\n\n  > ::: |\n  > %% c\n  b\n", "<p><br>\nb</p>"),
        ("> - ::: |\n>   {.k}\nb\n", "<p>{.k}<br>\nb</p>"),
        ("- - > ::: |\n    > {.k}\nb\n", "<p>{.k}<br>\nb</p>"),
        ("- > ::: |\n  > %%%\n  > c\n  > %%%\nb\n", "<p>b</p>"),
        ("> :::: note\n> ::: |\n> y\nb\n", "<p>y<br>\nb</p>"),
        ("- > :::: note\n  > ::: |\n  > y\nb\n", "<p>y<br>\nb</p>"),
    ] {
        let html = to_html(source);
        assert!(html.contains(stanza), "{source}\n{html}");
        assert!(
            html.ends_with("</ul>") || html.ends_with("</blockquote>"),
            "{source}\n{html}"
        );
    }
}

#[test]
fn a_trailing_attribute_shaped_payload_stays_in_the_stanza() {
    for (source, stanza) in [
        ("- x\n  ::: |\n  {.k}\n", "<p>{.k}</p>"),
        (
            "- x\n  ::: |\n  {.k}\n  y\n  {.j}\n",
            "<p>{.k}<br>\ny<br>\n{.j}</p>",
        ),
        ("> - x\n>   ::: |\n>   {.k}\n", "<p>{.k}</p>"),
    ] {
        let html = to_html(source);
        assert!(html.contains(stanza), "{source}\n{html}");
    }
}

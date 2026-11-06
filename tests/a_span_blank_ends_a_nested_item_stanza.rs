//! CARVE-P9-047: a blank inside an unclosed `%%%` span keeps a stanza open
//! only for a lazy follower, and never past a nested item (carve#5269).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_nested_quoted_item_stanza_ends_at_its_span_blank() {
    let html = html("> - a\n>   - ::: |\n>     x\n>     %%%\n>     \nb\n");
    assert!(
        html.trim_end()
            .ends_with("    </li>\n  </ul>\n  <p>b</p>\n</blockquote>"),
        "{html}"
    );
}

#[test]
fn a_nested_item_child_stanza_ends_at_its_span_blank() {
    let html = html("> - a\n>   - c\n>     ::: |\n>     x\n>     %%%\n>     \nb\n");
    assert!(html.contains("<p>x</p>"), "{html}");
    assert!(html.contains("<p>b</p>\n</blockquote>"), "{html}");
}

#[test]
fn an_unquoted_follower_below_the_item_leaves_the_list() {
    let html = html("- ::: |\n  x\n  %%%\n  \n y\n");
    assert!(html.trim_end().ends_with("</ul>\n<p>y</p>"), "{html}");
}

#[test]
fn a_quoted_item_stanza_still_takes_the_lazy_follower() {
    let html = html("> - ::: |\n>   x\n>   %%%\n>\nb\n");
    assert!(html.contains("<p>x<br>\nb</p>"), "{html}");
}

#[test]
fn without_a_blank_the_nested_stanza_still_takes_the_follower() {
    let html = html("> - a\n>   - ::: |\n>     x\n>     %%%\nb\n");
    assert!(html.contains("<p>x<br>\nb</p>"), "{html}");
}

#[test]
fn a_thousand_variants_follow_the_model() {
    for n in 0..1000 {
        let marker = ["- ", "1. ", "* "][n % 3];
        let pad = " ".repeat(marker.len());
        let blank = [">", ">     ", ">  "][(n / 3) % 3];
        let source =
            format!("> - a{n}\n>   {marker}::: |\n>   {pad}x{n}\n>   {pad}%%%\n{blank}\nb{n}\n");
        let html = html(&source);
        assert!(html.contains(&format!("<p>x{n}</p>")), "{html}");
        assert!(
            html.contains(&format!("<p>b{n}</p>\n</blockquote>")),
            "{html}"
        );
    }
}

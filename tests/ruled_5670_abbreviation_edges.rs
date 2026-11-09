use carve::to_html;

#[test]
fn abbreviation_boundary_crosses_eligible_inline_nodes() {
    assert_eq!(
        to_html("[w[HTML](u)]()\n\n*[HTML]: t\n").trim(),
        "<p><a href=\"\">wHTML</a></p>"
    );
    assert_eq!(
        to_html("w{*HTML*}\n\n*[HTML]: t\n").trim(),
        "<p>w<strong>HTML</strong></p>"
    );
    assert_eq!(
        to_html("`w`{*HTML*}\n\n*[HTML]: t\n").trim(),
        "<p><code>w</code><strong><abbr title=\"t\">HTML</abbr></strong></p>"
    );
    assert_eq!(
        to_html("[w`x`](u)HTML\n\n*[HTML]: t\n").trim(),
        "<p><a href=\"u\">w<code>x</code></a><abbr title=\"t\">HTML</abbr></p>"
    );
    assert_eq!(
        to_html("w{#c#}{*HTML*}\n\n*[HTML]: t\n").trim(),
        "<p>w<span class=\"critic-comment\">c</span><strong><abbr title=\"t\">HTML</abbr></strong></p>"
    );
    for shape in 0..2 {
        for shift in 0..500 {
            let left = if shift % 2 == 0 { "w" } else { "é" }.repeat(shift % 5 + 1);
            let (source, expected) = if shape == 0 {
                (
                    format!("{left}{{*HTML*}}\n\n*[HTML]: t\n"),
                    format!("<p>{left}<strong>HTML</strong></p>"),
                )
            } else {
                (
                    format!("[{left}[HTML](u)]()\n\n*[HTML]: t\n"),
                    format!("<p><a href=\"\">{left}HTML</a></p>"),
                )
            };
            assert_eq!(to_html(&source).trim(), expected, "{source:?}");
        }
    }
}

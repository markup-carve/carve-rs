use carve::to_html;

#[test]
fn a_later_closed_div_does_not_close_the_attached_div_in_a_note() {
    let html = to_html("[^1]: +\n::: x\n\n  ::: y\n  b\n  :::\n\n[^1]\n");
    assert!(html.contains("<div class=\"x\">\n\n      </div>"), "{html}");
    assert!(html.contains("<div class=\"y\">\n        <p>b</p>\n      </div>"), "{html}");
}

#[test]
fn a_later_closed_div_does_not_close_the_attached_div_in_a_description() {
    let html = to_html(":: t\n: +\n::: x\n\n  ::: y\n  b\n  :::\n");
    assert!(html.contains("<div class=\"x\">\n\n    </div>"), "{html}");
    assert!(html.contains("<div class=\"y\">\n      <p>b</p>\n    </div>"), "{html}");
}

#[test]
fn a_later_div_below_the_content_column_leaves_the_host() {
    for (source, end) in [
        ("- +\n::: x\n\n ::: y\n b\n :::\n", "</ul>\n<div class=\"y\">"),
        (
            ":: t\n: +\n::: x\n\n ::: y\n b\n :::\n",
            "</dl>\n<div class=\"y\">",
        ),
        (
            "[^1]: +\n::: x\n\n ::: y\n b\n :::\n\n[^1]\n",
            "<div class=\"y\">",
        ),
    ] {
        let html = to_html(source);
        if source.starts_with("[^1]") {
            assert!(html.starts_with(end), "{source:?}: {html}");
        } else {
            assert!(html.contains(end), "{source:?}: {html}");
        }
    }
}


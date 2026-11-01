//! A `: ` marker line in a quote whose body opens a block (an empty div, a
//! note holding one) leaves no paragraph for an unquoted line (carve-rs#3665).

fn html(src: &str) -> String {
    let out = carve::to_html(src);
    out.split("<section").next().unwrap().to_string()
}

#[test]
fn an_empty_note_div_in_a_quoted_description_ends_the_quote() {
    assert_eq!(
        html("r[^1]\n\n> :: t\n> : [^1]: :::\n    p\n    ::::\nz\n"),
        "<p>r<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd></dd>\n  </dl>\n</blockquote>\n<p>p</p>\n<div>\n  <p>z</p>\n</div>\n"
    );
}

#[test]
fn an_empty_div_in_a_quoted_description_ends_the_quote() {
    assert!(html("> :: t\n> : :::\np\n").ends_with("</blockquote>\n<p>p</p>"));
}

#[test]
fn a_description_paragraph_still_takes_the_lazy_line() {
    assert_eq!(html("> :: t\n> : x\np\n"), html("> :: t\n> : x\n>   p\n"));
}

#[test]
fn a_comment_body_keeps_the_old_reading() {
    assert!(html("> :: t\n> : %% c\np\n").ends_with("<p>p</p>\n</blockquote>"));
}

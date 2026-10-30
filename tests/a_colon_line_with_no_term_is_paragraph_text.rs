//! A `: ` line with no term above it is paragraph text, and a definition shape
//! behind its next `: ` line stays text too (PART 9 §24 C3, carve-rs#3534).

fn html(source: &str) -> String {
    carve::to_html(source).trim_end().to_string()
}

#[test]
fn a_definition_shape_after_a_termless_colon_line_stays_text() {
    for (source, expected) in [
        (": x\n: [r]: /u\n", "<p>: x\n: [r]: /u</p>"),
        (
            "[t][r]\n\n: x\n: [r]: /u\n",
            "<p>[t][r]</p>\n<p>: x\n: [r]: /u</p>",
        ),
        (
            ": x\n: [^n]: b\n\ny[^n]\n",
            "<p>: x\n: [^n]: b</p>\n<p>y[^n]</p>",
        ),
        (
            "> : x\n> : [r]: /u\n",
            "<blockquote><p>: x\n: [r]: /u</p></blockquote>",
        ),
        (
            "- : x\n  : [r]: /u\n",
            "<ul>\n  <li>: x\n: [r]: /u</li>\n</ul>",
        ),
        (
            ":: t\n: a\n\npara\n\n: x\n: [r]: /u\n",
            "<dl>\n  <dt>t</dt>\n  <dd>a</dd>\n</dl>\n<p>para</p>\n<p>: x\n: [r]: /u</p>",
        ),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

#[test]
fn fmt_keeps_the_text_and_invents_no_definition() {
    let source = "a [t][r] b\n\n: [r]: /u\n: [r]: /u\n";
    assert_eq!(carve::to_carve(source), source);
}

#[test]
fn a_definition_in_a_description_under_a_term_still_registers() {
    for source in [
        ":: t\n: a\n: [r]: /u\n\n[x][r]\n",
        ":: t\n\n: a\n\n: [r]: /u\n\n[x][r]\n",
        ":: t\nmore\n: a\n: [r]: /u\n\n[x][r]\n",
        "> :: t\n> : a\n> : [r]: /u\n\n[x][r]\n",
    ] {
        assert!(html(source).contains("<a href=\"/u\">x</a>"), "{source:?}");
    }
}

// carve#4283: a bullet line after a hostless caption line owns nothing.
#[test]
fn an_indented_block_after_a_caret_and_bullet_line_opens() {
    assert_eq!(
        html("^ p\n- x\n  > q\n"),
        "<p>^ p\n- x</p>\n<blockquote><p>q</p></blockquote>"
    );
    assert_eq!(html("- ^ c\n 1. x\n"), "<ul>\n  <li>^ c\n1. x</li>\n</ul>");
}

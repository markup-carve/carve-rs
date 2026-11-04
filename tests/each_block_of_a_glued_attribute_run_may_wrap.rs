//! carve#4078, superseding ruling: each block of a glued run keeps a single
//! block's wrapping rule.

use carve::to_html;

#[test]
fn a_break_inside_any_block_keeps_the_run() {
    for source in [
        // carve#5080: each wrapped line sits past its block's `{`.
        "{.a}{.b\n     }\np\n",
        "{.a\n }{.b}\np\n",
        "{.a\n }{.b\n   }\np\n",
        " {.a}{.b\n      }\np\n",
    ] {
        assert_eq!(
            to_html(source).trim(),
            r#"<p class="a b">p</p>"#,
            "{source}"
        );
    }
}

#[test]
fn the_run_ends_a_paragraph_and_nests() {
    assert_eq!(
        to_html("para\n{.a}{.b\n}\nq\n").trim(),
        "<p>para\n{.a}{.b\n}\nq</p>"
    );
    assert_eq!(
        to_html("> {.a\n> }{.b}\n> p\n").trim(),
        "<blockquote><p>{.a\n}{.b}\np</p></blockquote>"
    );
}

#[test]
fn a_run_with_nothing_below_renders_nothing() {
    assert_eq!(to_html("{.a}{.b\n}\n").trim(), "<p>{.a}{.b\n}</p>");
}

#[test]
fn a_broken_run_stays_text() {
    for source in ["{.a\n}x{.b}\np\n", "{.a}{.b\n}x\np\n", "{k=\"x\n}{.b}\np\n"] {
        assert!(to_html(source).trim().starts_with("<p>{"), "{source}");
    }
}

//! Ports of the current brace-run and own-line fence rulings.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_owned()
}

#[test]
fn refused_brace_openers_leave_later_lines_to_their_host() {
    assert_eq!(
        html("{.x\n.y}\n# H\n"),
        "<p>{.x\n.y}</p>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
    assert_eq!(
        html("- {.x\n }\n  - q\n"),
        "<ul>\n  <li>{.x\n}\n    <ul>\n      <li>q</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        html(":: t\n:   {% a\n     {k=v}\n: next\nz %} y\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>{% a</dd>\n  <dd>next\nz %} y</dd>\n</dl>"
    );
}

#[test]
fn authored_private_use_text_survives_a_refused_opener() {
    assert_eq!(
        html("{.x\n.y}\n\u{e000}\u{e001}\n"),
        "<p>{.x\n.y}\n\u{e000}\u{e001}</p>"
    );
}

#[test]
fn wrapped_quote_run_requires_each_prefix() {
    assert_eq!(
        html("> {.x\n>   .y}\n> # H\n"),
        "<blockquote>\n  <h1 class=\"x y\" id=\"H\">H</h1>\n</blockquote>"
    );
    assert_eq!(
        html("> > - {.x\n> >    .y}\nplain\n"),
        "<blockquote>\n  <blockquote>\n    <ul>\n      <li></li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>plain</p>"
    );
}

#[test]
fn quoted_run_in_note_uses_the_written_brace_column() {
    let source = "[^q]: > {.x\n     .y}\n\nx[^q]\n";
    assert!(
        html(source).contains("<blockquote><p>{.x\n.y}</p></blockquote>"),
        "{}",
        html(source)
    );
}

#[test]
fn own_line_fence_payload_keeps_spaces_after_a_blank() {
    for source in [
        "[^a]: p\n+\n```\ncode\n\n  tail\n\n```\n\nx[^a]\n",
        ":: t\n: p\n+\n```\ncode\n\n  tail\n\n```\n",
    ] {
        assert!(
            html(source).contains("<pre><code>code\n\n  tail\n\n</code></pre>"),
            "{}",
            html(source)
        );
    }
}

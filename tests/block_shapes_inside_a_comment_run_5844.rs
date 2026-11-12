use carve::to_html;

// carve#5844: a valid `{%` run holds its block-shaped continuations, so the item stays tight.
#[test]
fn a_blank_separated_run_hides_every_block_shape() {
    let hosts = [("- p", 2, "ul"), ("1. p", 3, "ol"), ("- [ ] p", 6, "ul")];
    let middles = [
        "m", "- m", "1. m", "---", "# m", "> m", "| a |", "{.c}", "::: x", "```",
    ];
    for (head, col, tag) in hosts {
        for middle in middles {
            for shift in [1, 2] {
                let pad = " ".repeat(col);
                let inner = " ".repeat(col + shift);
                let source = format!("{head}\n\n{pad}{{% a\n{inner}{middle}\n{inner}z %}}\n");
                let html = to_html(&source);
                assert!(
                    !html.contains("<p>") && html.matches(&format!("<{tag}")).count() == 1,
                    "{source}\n{html}"
                );
            }
        }
    }
}

#[test]
fn a_quoted_item_run_hides_a_thematic_break() {
    assert_eq!(
        to_html("> - p\n>\n>   {% a\n>    ---\n>    z %}\n"),
        "<blockquote>\n  <ul>\n    <li>p</li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn a_run_continuing_the_lead_holds_a_marker() {
    assert_eq!(
        to_html("- p\n  {% a\n   - m\n   z %}\n- q\n"),
        "<ul>\n  <li>p\n</li>\n  <li>q</li>\n</ul>"
    );
}

#[test]
fn a_marker_at_the_brace_column_still_opens_its_item() {
    let html = to_html("- p\n\n  {% a\n  - m\n  z %}\n");
    assert!(html.contains("<li>m"), "{html}");
}

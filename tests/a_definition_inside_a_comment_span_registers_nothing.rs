//! A link or abbreviation definition inside a `%%%` span is comment text and
//! registers nothing (corpus 0190), also where a `+` skips the span or a note
//! body holds it (markup-carve/carve#5151).

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

const FOLLOWER: &str = "<p>See [r][] and HTML.</p>";

#[test]
fn a_span_a_plus_skips_registers_nothing() {
    for host in ["- a", "1. a", "> a", "- a\n  - b"] {
        for fence in ["%%%", "%%%%"] {
            let source =
                format!("{host}\n+\n{fence}\n[r]: /u\n*[HTML]: Hyper\n{fence}\n\nSee [r][] and HTML.\n");
            let html = flat(&source);
            assert!(html.ends_with(FOLLOWER), "{source:?}\n{html}");
        }
    }
}

#[test]
fn the_ticket_reproduction() {
    assert_eq!(
        flat("- a\n+\n%%%\n[r]: /u\n%%%\n\nSee [r][].\n"),
        "<ul><li>a</li></ul><p>See [r][].</p>"
    );
}

#[test]
fn a_span_in_a_note_body_registers_nothing() {
    for gap in ["", "\n"] {
        for indent in ["", " ", "  ", "    "] {
            let source = format!(
                "See[^n].\n\n[^n]: x\n{gap}{indent}%%%\n{indent}[r]: /u\n{indent}%%%\n\nSee [r][] and HTML.\n"
            );
            let html = flat(&source);
            assert!(html.contains(FOLLOWER), "{source:?}\n{html}");
        }
    }
}

#[test]
fn a_definition_after_the_span_still_registers() {
    assert_eq!(
        flat("- a\n+\n%%%\n[r]: /u\n%%%\n[s]: /v\npara\n\nSee [r][] and [s][].\n"),
        "<ul><li>apara</li></ul><p>See [r][] and <a href=\"/v\">s</a>.</p>"
    );
    let html = flat("See[^n].\n\n[^n]: x\n  %%%\n  [r]: /u\n  %%%\n  [s]: /v\n\nSee [r][] and [s][].\n");
    assert!(
        html.contains("<p>See [r][] and <a href=\"/v\">s</a>.</p>"),
        "{html}"
    );
}

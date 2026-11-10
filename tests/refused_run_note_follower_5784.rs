use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn a_note_definition_after_a_refused_run_stays_in_its_open_paragraph() {
    for (source, expected) in [
        (
            ":: t\n: {.x\n}\n[^m]: n\nz\n",
            "<dl>\n  <dt>t</dt>\n  <dd>{.x\n}\n[^m]: n\nz</dd>\n</dl>",
        ),
        (
            "- {.x\n}\n[^m]: n\nz\n",
            "<ul>\n  <li>{.x\n}\n[^m]: n\nz</li>\n</ul>",
        ),
        (
            "- :: t\n  : {.x\n}\n[^m]: n\nz\n",
            "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>{.x\n}\n[^m]: n\nz</dd>\n    </dl>\n  </li>\n</ul>",
        ),
    ] {
        check(source, expected);
    }
}

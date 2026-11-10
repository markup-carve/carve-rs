use carve::{to_html, to_html_with_options, Options};

#[test]
fn list_shaped_follower_of_a_refused_description_comment_is_text() {
    for (source, expected) in [
        (
            ":: t\n: {% a\n%}\n- next\n",
            "<dl>\n  <dt>t</dt>\n  <dd>{% a\n%}\n- next</dd>\n</dl>",
        ),
        (
            ":: t\n:  {% a\n  %}\n1. next\n",
            "<dl>\n  <dt>t</dt>\n  <dd>{% a\n%}\n1. next</dd>\n</dl>",
        ),
    ] {
        assert_eq!(to_html(source), expected);
        assert_eq!(
            to_html_with_options(source, &Options::default().with_positions(true)),
            expected
        );
    }
}

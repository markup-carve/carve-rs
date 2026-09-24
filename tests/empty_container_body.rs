use carve::{to_ansi, to_carve, to_html, to_markdown, to_plain_text};

const EMPTY_CONTAINERS: &[(&str, &str)] = &[
    (":::\n:::\n", "<div>\n\n</div>"),
    (":::\n%% hidden\n:::\n", "<div>\n\n</div>"),
    ("::: |\n:::\n", "<div class=\"line-block\">\n\n</div>"),
    ("::: \\\n:::\n", "<div class=\"hardbreaks\">\n\n</div>"),
    (
        "::: \\\n%% hidden\n:::\n",
        "<div class=\"hardbreaks\">\n\n</div>",
    ),
    (
        "::: figure\n:::\n",
        "<figure class=\"carve-figure-group\">\n\n</figure>",
    ),
    (
        "::: figure\n%% hidden\n:::\n",
        "<figure class=\"carve-figure-group\">\n\n</figure>",
    ),
];

#[test]
fn an_empty_bare_div_keeps_its_html_body_line() {
    for &(source, expected) in &EMPTY_CONTAINERS[..2] {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}

#[test]
fn every_empty_special_container_keeps_its_html_body_line() {
    for &(source, expected) in &EMPTY_CONTAINERS[2..] {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}

#[test]
fn wrapperless_targets_add_no_visible_content() {
    for &(source, _) in EMPTY_CONTAINERS {
        assert_eq!(to_markdown(source), to_markdown(""), "{source:?}");
        assert_eq!(to_plain_text(source), to_plain_text(""), "{source:?}");
        assert_eq!(to_ansi(source), to_ansi(""), "{source:?}");
    }
}

#[test]
fn the_canonical_writer_preserves_the_structure_and_authored_comments() {
    for &(source, _) in EMPTY_CONTAINERS {
        let canonical = to_carve(source);
        assert_eq!(to_html(&canonical), to_html(source), "{source:?}");
        assert_eq!(to_carve(&canonical), canonical, "{source:?}");
        if source.contains("%% hidden") {
            assert!(canonical.contains("%% hidden"), "{source:?}");
        }
    }
}

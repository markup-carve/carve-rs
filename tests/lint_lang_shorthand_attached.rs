//! carve#6969: the parse lifts `{:fr}` into `lang` (C2-A-027), so the attribute line attaches.

use carve::lint::{LintOptions, lint};
use carve::{Options, render};

fn codes(source: &str) -> Vec<&'static str> {
    lint(source, &LintOptions::default())
        .into_iter()
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn a_lang_only_attribute_line_attaches() {
    for source in [
        "{:fr}\nUn paragraphe.\n",
        "> {:fr}\n> Un paragraphe.\n",
        "- {:fr}\n  Un paragraphe.\n",
        "{:fr}\n# Titre\n",
        "{:fr}\n- item\n",
        "{:fr}\n> cite\n",
        "{:fr}\n| a | b |\n",
    ] {
        assert_eq!(codes(source), Vec::<&str>::new(), "{source:?}");
    }
}

#[test]
fn a_lang_only_attribute_line_that_reaches_no_block_still_reports() {
    for source in ["{:fr}\n", "{:fr}\n\nUn paragraphe.\n"] {
        assert_eq!(codes(source), ["unattached-block-attribute"], "{source:?}");
    }
}

#[test]
fn render_keeps_the_lang_attribute() {
    assert_eq!(
        render("{:fr}\nUn paragraphe.\n", &Options::default()).trim_end(),
        "<p lang=\"fr\">Un paragraphe.</p>"
    );
}

//! A region marker below a line-block stanza (carve-rs#4675), the anonymous div
//! further-words message (carve-rs#4637), a comment line above a typed opener
//! (carve-rs#4612) and an image in a container title (carve-rs#4605).

use carve::lint::{LintOptions, lint};
use carve::{Options, render};

const STANZA_END: &str = "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>a</p>\n    </div>\n  </li>\n</ul>\n<p>y</p>";

#[test]
fn c2_b_024_a_region_marker_line_ends_a_lazy_stanza() {
    let opts = Options::default();
    assert_eq!(render("- ::: |\n  a\n%% #region a\ny\n", &opts), STANZA_END);
    assert_eq!(
        render("- ::: |\n  a\n  %%%\n%% #region a\ny\n", &opts),
        STANZA_END
    );
    assert_eq!(
        render("> ::: |\n> a\n%% #region a\ny\n", &opts),
        "<blockquote>\n  <div class=\"line-block\">\n    <p>a</p>\n  </div>\n</blockquote>\n<p>y</p>"
    );
}

fn title_findings(source: &str) -> Vec<(u32, String)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|d| d.code == "fence-title-syntax")
        .map(|d| (d.pos.map_or(0, |p| p.start_column), d.message))
        .collect()
}

const ANONYMOUS: &str = "These words are not part of the anonymous div opener; it opens without them. Remove the words or use a named div.";

#[test]
fn c2_b_119_an_anonymous_div_opener_gets_its_message_at_any_run_and_column() {
    assert_eq!(
        title_findings(":::: *\nx\n::::\n"),
        [(6, ANONYMOUS.to_string())]
    );
    assert_eq!(
        title_findings("- ::: *\n  x\n  :::\n"),
        [(7, ANONYMOUS.to_string())]
    );
    assert_eq!(
        title_findings("> ::: \"t\" x\n> x\n> :::\n"),
        [(11, ANONYMOUS.to_string())]
    );
}

#[test]
fn c2_b_122_the_corrected_opener_leaves_the_list_marker_out() {
    assert_eq!(
        title_findings("- ::: note x\n  y\n  :::\n"),
        [(
            12,
            "These further words are not read as a title, label or attributes. The block opens without them. Use \"::: note\" for the same block with only valid opener parts; put attributes on their own line above it.".to_string()
        )]
    );
}

#[test]
fn c2_b_137_a_comment_line_above_a_typed_opener_keeps_the_blank_line() {
    let opts = Options::default();
    assert_eq!(render("a\n\n%% c\n---yaml\nk: v\n---\n", &opts), "<p>a</p>");
    assert_eq!(
        render("---yaml\na: 1\n---\n\n%% c\n---yaml\nk: v\n---\n", &opts),
        ""
    );
}

#[test]
fn c2_b_120_an_image_in_a_container_title_is_no_overindented_block() {
    for source in [
        "- ::: \"![i](/u)\"\n",
        "1. ::: \"![i](/u)\"\n",
        "1. :::: \"![i][r]\"\n",
    ] {
        let codes: Vec<_> = lint(source, &LintOptions::default())
            .into_iter()
            .map(|d| d.code)
            .collect();
        assert!(
            !codes.contains(&"list-item-block-overindented"),
            "{source}: {codes:?}"
        );
        assert!(codes.contains(&"unclosed-container-fence"), "{source}");
    }
}

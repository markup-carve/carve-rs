//! markup-carve/carve#6459, #6479, #6429 and #6444 on next2.

use carve::lint::{LintOptions, lint};

fn html(src: &str) -> String {
    carve::render(src, &carve::Options::default())
}

fn types(src: &str) -> Vec<String> {
    carve::parse(src)
        .unwrap()
        .children
        .iter()
        .map(|c| c.type_name().to_string())
        .collect()
}

fn comment_fences(src: &str) -> Vec<(u32, u32)> {
    lint(src, &LintOptions::default())
        .into_iter()
        .filter(|d| d.code == "unclosed-comment-fence")
        .filter_map(|d| d.pos.map(|p| (p.start_offset, p.end_offset)))
        .collect()
}

const BACKLINK: &str =
    "<p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>";

#[test]
fn an_inline_footnote_of_only_comment_lines_writes_the_backlink_alone() {
    for src in [
        "^[\n%% c\n](u) z\n",
        "^[\\\n%% c\n](u) z\n",
        "- ^[\\\n  %% c\n  ](u) z\n",
        "> ^[\n> %% c\n> %% d\n> ](u) z\n",
        "y ^[\n%% c\n\\\n%% d\n] z\n",
    ] {
        let out = html(src);
        assert!(
            out.contains(&format!("<li id=\"fn1\">\n      {BACKLINK}")),
            "{src:?}\n{out}"
        );
    }
}

#[test]
fn an_inline_footnote_with_text_keeps_it_before_the_backlink() {
    let out = html("^[a\n%% c\n](u) z\n");
    assert!(out.contains("<p>a\n<a href=\"#fnref1\""), "{out}");
}

#[test]
fn a_comment_block_its_line_block_closes_is_reported_over_its_body() {
    assert_eq!(comment_fences("::: |\na\n%%%\nc\n:::\n"), [(8, 13)]);
    assert_eq!(
        comment_fences("- ::: |\n  a\n  %%%\n  c\n  :::\n"),
        [(14, 21)]
    );
}

#[test]
fn a_comment_block_open_at_the_end_of_an_unclosed_line_block_is_reported_once() {
    let codes: Vec<&str> = lint("::: |\na\n%%%\nc\n", &LintOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(
        codes,
        ["unclosed-container-fence", "unclosed-comment-fence"]
    );
}

#[test]
fn a_closed_comment_block_in_a_line_block_is_not_reported() {
    assert!(comment_fences("::: |\na\n%%%\nc\n%%%\nb\n:::\n").is_empty());
}

#[test]
fn a_typed_opener_after_a_description_list_or_footnote_opens_a_metadata_block() {
    assert_eq!(
        types(":: t\n: d\n\n---yaml\nk: v\n---\n"),
        ["definition_list", "frontmatter"]
    );
    assert_eq!(
        types("x[^n]\n\n[^n]: y\n\n---yaml\nk: v\n---\n"),
        ["paragraph", "footnote", "frontmatter"]
    );
    assert_eq!(
        html(":: t\n: d\n\n---yaml\nk: v\n---\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn a_shortcut_reference_in_a_cell_is_keyed_after_the_pipe_escape() {
    let linked = html("| [r\\|s] |\n\n[r|s]: /u\n");
    assert!(
        linked.contains("<td><a href=\"/u\">r|s</a></td>"),
        "{linked}"
    );
    let text = html("| [r\\|s] |\n\n[r\\|s]: /u\n");
    assert!(text.contains("<td>[r|s]</td>"), "{text}");
}

//! A list item or definition whose whole body was COLLECTED -- a link
//! reference definition, a footnote definition -- leaves its marker alone on
//! the line. The Markdown writer used to keep the marker's separator space
//! there, so the line ended in whitespace.
//!
//! Nothing in the corpus pins that: across all 51 `.md` and `.fmt` goldens no
//! line ends in a space, and the list writer's own continuation pad already
//! refuses to pad an empty line for exactly this reason -- trailing whitespace
//! is what editors and `git apply --whitespace=fix` rewrite behind the writer.
//! carve-js writes the marker bare; this brings carve-rs in line with it.

use carve::to_markdown;

fn lines(rendered: &str) -> Vec<&str> {
    rendered.lines().collect()
}

#[test]
fn a_list_item_holding_only_a_reference_definition_writes_the_bullet_bare() {
    let out = to_markdown("- [ref]: /url\n\nSee [it][ref].\n");
    assert_eq!(lines(&out)[0], "-");
    assert!(
        !out.contains("- \n"),
        "no line ends in the marker's separator: {out:?}"
    );
}

#[test]
fn a_list_item_holding_only_a_footnote_definition_writes_the_bullet_bare() {
    let out = to_markdown("See [^a].\n\n- [^a]: note body\n");
    assert!(lines(&out).contains(&"-"));
    assert!(
        !out.contains("- \n"),
        "no line ends in the marker's separator: {out:?}"
    );
}

#[test]
fn a_definition_holding_only_a_reference_definition_writes_the_colon_bare() {
    let out = to_markdown(":: term\n:  [r]: /u\n\nsee [t][r]\n");
    assert!(lines(&out).contains(&":"));
    assert!(
        !out.contains(": \n"),
        "no line ends in the marker's separator: {out:?}"
    );
}

#[test]
fn a_task_item_with_no_content_keeps_its_box_and_drops_the_space() {
    let out = to_markdown("- [ ] [ref]: /url\n\nSee [it][ref].\n");
    assert_eq!(lines(&out)[0], "- [ ]");
}

#[test]
fn an_item_that_still_has_content_keeps_the_separator() {
    let out = to_markdown("- x\n");
    assert_eq!(lines(&out)[0], "- x");
}

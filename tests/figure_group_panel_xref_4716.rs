//! carve-rs#4716, C2-A-084, C2-H-095: the panels of a figure group are its
//! tables and the image paragraphs, code blocks, quotes and math blocks that
//! carry a caption; only panels are cross-reference targets and take letters.

fn see(src: &str) -> String {
    let out = carve::render(src, &carve::Options::default());
    out.lines()
        .find(|l| l.starts_with("<p>See "))
        .unwrap_or_else(|| panic!("no reference line in {out}"))
        .to_string()
}

fn group(children: &[&str]) -> String {
    format!(
        "::: figure\n{}\n:::\n^ Figure #: Group\n\nSee </#a> and </#b>.\n",
        children.join("\n\n")
    )
}

const KINDS: [(&str, &str); 5] = [
    ("image", "![x](x.png)"),
    ("fence", "```\ncode\n```"),
    ("quote", "> q"),
    ("math", "$$`x`"),
    ("paragraph", "some text"),
];

#[test]
fn an_uncaptioned_child_is_no_panel() {
    for (kind, body) in KINDS {
        let src = group(&[&format!("{{#a}}\n{body}"), &format!("{{#b}}\n{body}")]);
        assert_eq!(
            see(&src),
            "<p>See &lt;/#a&gt; and &lt;/#b&gt;.</p>",
            "{kind}: {src}"
        );
    }
}

#[test]
fn a_captioned_child_is_a_panel() {
    for (kind, body) in KINDS.iter().filter(|(k, _)| *k != "paragraph") {
        let src = group(&[
            &format!("{{#a}}\n{body}\n^ (a) One"),
            &format!("{{#b}}\n{body}\n^ (b) Two"),
        ]);
        assert_eq!(
            see(&src),
            "<p>See <a href=\"#a\">Figure 1a</a> and <a href=\"#b\">Figure 1b</a>.</p>",
            "{kind}: {src}"
        );
    }
}

#[test]
fn an_uncaptioned_child_takes_no_letter() {
    for (kind, body) in KINDS {
        let src = group(&[&format!("{{#a}}\n{body}"), "{#b}\n![y](y.png)\n^ (a) One"]);
        assert_eq!(
            see(&src),
            "<p>See &lt;/#a&gt; and <a href=\"#b\">Figure 1a</a>.</p>",
            "{kind}: {src}"
        );
    }
}

#[test]
fn a_table_is_a_panel_without_a_caption() {
    let src = group(&["{#a}\n| x |", "{#b}\n![y](y.png)\n^ (b) Two"]);
    assert_eq!(
        see(&src),
        "<p>See <a href=\"#a\">Figure 1a</a> and <a href=\"#b\">Figure 1b</a>.</p>"
    );
}

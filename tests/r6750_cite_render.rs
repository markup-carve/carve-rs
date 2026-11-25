//! carve#6750: `render --extensions '["carve.cite"]'` renders citations with
//! the standard citations renderer, byte-identical to carve-js (C2-X-058).
use std::path::PathBuf;
use std::process::Command;

fn render(tag: &str, source: &str, extensions: bool) -> String {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("carve-rs-6750-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.crv"), source).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_carve"));
    cmd.arg("render");
    if extensions {
        cmd.args(["--extensions", r#"["carve.cite"]"#]);
    }
    let out = cmd.arg("a.crv").current_dir(&dir).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .trim_end_matches('\n')
        .to_string()
}

#[test]
fn a_resolved_citation_lists_every_entry_of_its_directive() {
    assert_eq!(
        render(
            "a",
            "See :cite[doe].\n\n::: references \"References\"\n:: doe\n: Doe, J. (2020). /A book/.\n\n:: roe\n: Roe, R. (2019). Paper.\n:::\n",
            true
        ),
        "<p>See [<a href=\"#ref-doe\" data-cite-key=\"doe\">1</a>].</p>\n<div class=\"references\">\n  <p class=\"admonition-title\">References</p>\n  <ol class=\"references\">\n    <li id=\"ref-doe\">Doe, J. (2020). <em>A book</em>.</li>\n    <li id=\"ref-roe\">Roe, R. (2019). Paper.</li>\n  </ol>\n</div>"
    );
}

#[test]
fn every_directive_keeps_its_entries_and_a_later_list_starts_at_its_number() {
    assert_eq!(
        render(
            "b",
            "::: references\n:: roe\n:: doe\n: Shared.\n:::\n\n::: references\n:: z\n: Unused.\n:::\n\nSee :cite[doe] and :cite[nope].\n",
            true
        ),
        "<div class=\"references\">\n  <ol class=\"references\">\n    <li id=\"ref-roe\">Shared.</li>\n    <li id=\"ref-doe\">Shared.</li>\n  </ol>\n</div>\n<div class=\"references\">\n  <ol start=\"3\" class=\"references\">\n    <li id=\"ref-z\">Unused.</li>\n  </ol>\n</div>\n<p>See [<a href=\"#ref-doe\" data-cite-key=\"doe\">2</a>] and nope.</p>"
    );
}

#[test]
fn a_duplicate_key_takes_its_last_definition() {
    assert_eq!(
        render(
            "c",
            "See :cite[a].\n\n::: references\n:: a\n: Old.\n\n:: b\n: Other.\n\n:: a\n: New.\n:::\n",
            true
        ),
        "<p>See [<a href=\"#ref-a\" data-cite-key=\"a\">2</a>].</p>\n<div class=\"references\">\n  <ol class=\"references\">\n    <li id=\"ref-b\">Other.</li>\n    <li id=\"ref-a\">New.</li>\n  </ol>\n</div>"
    );
}

#[test]
fn an_unknown_key_keeps_its_fallback() {
    assert_eq!(render("fallback", ":cite[doe]\n", true), "<p>doe</p>");
}

#[test]
fn without_the_flag_the_output_does_not_change() {
    assert_eq!(
        render("plain", "See :cite[doe].\n", false),
        "<p>See <span class=\"ext-cite\">doe</span>.</p>"
    );
}

//! Markdown, GFM and Djot import through the HTML hub (carve-rs#4344,
//! carve-js#5074). Expected sources are those carve-js `next2` writes.

use carve::api::{self, ImportFormat, ImportOptions, LossOptions, json::call};
use serde_json::{Value, json};

fn import(from: &str, source: &str) -> Value {
    let out: Value = serde_json::from_str(&call(
        &json!({"call": "importDocument", "from": from, "source": source}).to_string(),
    ))
    .unwrap();
    out.get("ok")
        .cloned()
        .unwrap_or_else(|| panic!("{from} {source:?}: {out}"))
}

fn carve(from: &str, source: &str) -> String {
    import(from, source)["source"].as_str().unwrap().to_string()
}

#[test]
fn markdown_and_gfm() {
    let cases = [
        (
            "markdown",
            "# Title\n\nSome *em* and **strong** and `code`.\n",
            "= Title\n\nSome /em/ and *strong* and `code`.\n",
        ),
        ("markdown", "- a\n\n  b\n- c\n", "- a\n\n  b\n\n- c\n"),
        (
            "markdown",
            "<div class=\"x\">raw</div>\n\ntext <b>bold</b> end\n",
            "```=html\n<div class=\"x\">raw</div>\n```\n\ntext `<b>`{=html}bold`</b>`{=html} end\n",
        ),
        (
            "markdown",
            "---\ntitle: T\n---\n\nbody\n",
            "---\ntitle: T\n---\n\nbody\n",
        ),
        (
            "markdown",
            "+++\ntitle = \"T\"\n+++\n\nbody\n",
            "---toml\ntitle = \"T\"\n---\n\nbody\n",
        ),
        (
            "markdown",
            "inline $x^2$ math\n\n$$\ny = 1\n$$\n",
            "inline $`x^2` math\n\n$$`y = 1`\n",
        ),
        (
            "markdown",
            "> [!NOTE]\n> Be careful.\n",
            "> [!NOTE]\n> Be careful.\n",
        ),
        (
            "gfm",
            "> [!NOTE]\n> Be careful.\n",
            "::: note\nBe careful.\n:::\n",
        ),
        (
            "gfm",
            "- [ ] todo\n- [x] done\n",
            "- [ ] todo\n- [x] done\n",
        ),
        (
            "gfm",
            "| a | b |\n|:--|--:|\n| 1 | 2 |\n",
            "|={<} a |={>} b |\n| 1 | 2 |\n",
        ),
        (
            "gfm",
            "Text[^1]\n\n[^1]: The note.\n",
            "Text[^1]\n\n[^1]: The note.\n",
        ),
    ];
    for (from, input, want) in cases {
        assert_eq!(carve(from, input), want, "{from} {input:?}");
    }
}

#[test]
fn djot() {
    let cases = [
        ("0. zero\n1. one\n", "{start=0}\n. zero\n. one\n"),
        (
            "``` =html\n<b>raw</b>\n```\n",
            "```=html\n<b>raw</b>\n```\n",
        ),
        (
            "inline `<i>x</i>`{=html} raw\n",
            "inline `<i>x</i>`{=html} raw\n",
        ),
        (
            "{#custom}\n# Heading\n\ntext\n",
            "{#custom}\n= Heading\n\ntext\n",
        ),
        ("# Generated\n\ntext\n", "= Generated\n\ntext\n"),
        ("- a\n\n- b\n", "- a\n\n- b\n"),
    ];
    for (input, want) in cases {
        assert_eq!(carve("djot", input), want, "{input:?}");
    }
}

#[test]
fn raw_blocks_keep_their_html_and_format() {
    let tree = &import("markdown", "<div>\nx\n</div>\n")["tree"];
    assert_eq!(tree["children"][0]["type"], "raw_block");
    assert_eq!(tree["children"][0]["format"], "html");
    assert_eq!(tree["children"][0]["value"], "<div>\nx\n</div>\n");
    let tree = &import("djot", "``` =latex\n\\x\n```\n")["tree"];
    assert_eq!(tree["children"][0]["format"], "latex");
}

#[test]
fn hub_findings_carry_no_range() {
    let r = import(
        "markdown",
        "<p onclick=\"x()\">a</p>\n\n<span style=\"x\">b</span> c\n",
    );
    for d in r["diagnostics"].as_array().unwrap() {
        if d["code"].as_str().unwrap().starts_with("import-") {
            assert!(d.get("range").is_none(), "{d}");
        }
        assert_ne!(d["code"], "import-whitespace-normalized");
    }
}

#[test]
fn rust_and_json_errors() {
    let imported = api::Carve::default()
        .import_from(
            "*a*",
            ImportFormat::Djot,
            &ImportOptions::default(),
            &LossOptions::default(),
        )
        .unwrap();
    assert_eq!(imported.source, "*a*\n");
    assert_eq!(ImportFormat::from_name("bbcode"), None);
    let out: Value = serde_json::from_str(&call(
        r#"{"call":"importDocument","from":"bbcode","source":"x"}"#,
    ))
    .unwrap();
    assert_eq!(out["error"]["path"], "/from");
}

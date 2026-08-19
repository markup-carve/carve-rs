//! SELF-REGRESSION snapshots for the non-HTML renderers: this engine's own
//! Markdown / PlainText / ANSI output, captured per case in
//! tests/fixtures/golden/<name>.{md,plain,ansi}, which it must keep
//! reproducing byte-for-byte.
//!
//! IT DOES NOT CHECK CROSS-ENGINE AGREEMENT, and it used to say it did: the
//! header called the golden "carve-php's output" and concluded "so all three
//! impls agree". Nothing here invokes carve-php or carve-js. A committed
//! snapshot cannot enforce a statement about another engine in either
//! direction - carve-php could move away from it with nothing failing, and the
//! file could be regenerated from this engine with nothing checking that
//! carve-php still produces it. Both happened: carve-php ran a block image
//! into the following paragraph on the plain and ANSI targets while these
//! tests stayed green, and the twin copy in carve-js had drifted to a
//! different case list and different inputs under the same names
//! (carve-rs#692, carve-js#762).
//!
//! THE THREE-WAY PROPERTY LIVES IN `npm run compare:impls` in the spec repo,
//! which runs markdown, plain and ansi through all three engines over the
//! whole corpus and reports engine-to-engine diffs per target. That is a gate
//! that can fail, and it is where a shape has to be COVERED for the property
//! to mean anything - the block-image case above was invisible to it only
//! because no corpus document had a block image followed by another block
//! (fixed in markup-carve/carve#849).
//!
//! So: keep adding cases here to pin THIS engine against its own regressions.
//! To assert that the engines agree, put the shape in the spec corpus.

use std::fs;
use std::path::{Path, PathBuf};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden")
}

fn case_names() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(golden_dir())
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            if p.extension().and_then(|s| s.to_str()) == Some("crv") {
                p.file_stem().map(|s| s.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

fn read(name: &str, ext: &str) -> String {
    fs::read_to_string(golden_dir().join(format!("{name}.{ext}"))).unwrap()
}

fn check(ext: &str, render: impl Fn(&str) -> String) {
    let mut failures = Vec::new();
    for name in case_names() {
        let input = read(&name, "crv");
        let expected = read(&name, ext);
        let actual = render(&input);
        if actual != expected {
            failures.push(format!(
                "case `{name}` ({ext}):\n--- expected ---\n{expected:?}\n--- actual ---\n{actual:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn markdown_parity() {
    check("md", carve::to_markdown);
}

#[test]
fn plain_text_parity() {
    check("plain", carve::to_plain_text);
}

#[test]
fn ansi_parity() {
    check("ansi", carve::to_ansi);
}

#[test]
fn markdown_code_fence_keeps_quoted_header() {
    assert_eq!(
        carve::to_markdown("```js \"Title\"\nx\n```"),
        "```js \"Title\"\nx\n```\n"
    );
}

#[test]
fn markdown_critic_delete_renders_as_del_element() {
    assert_eq!(carve::to_markdown("{-del-}"), "<del>del</del>\n");
    assert_eq!(carve::to_markdown("{+ins+}"), "<ins>ins</ins>\n");
}

#[test]
fn plain_text_links_render_visible_text_not_destination() {
    assert_eq!(carve::to_plain_text("[t](u)"), "t\n");
    assert_eq!(carve::to_plain_text("[t](u \"ti\")"), "t\n");
    assert_eq!(carve::to_plain_text("[a][r]\n\n[r]: /u \"T\""), "a\n");
    assert_eq!(carve::to_plain_text("<https://x>"), "https://x\n");
}

#[test]
fn plain_text_and_ansi_preserve_literal_nbsp() {
    let input = "#\u{00a0}h";
    let expected = "#\u{00a0}h\n";

    assert_eq!(carve::to_plain_text(input), expected);
    assert_eq!(carve::to_plain_text(input).as_bytes(), b"#\xc2\xa0h\n");
    assert_eq!(carve::to_ansi(input), expected);
    assert_eq!(carve::to_ansi(input).as_bytes(), b"#\xc2\xa0h\n");
}

#[test]
fn generated_nbsp_renders_as_ascii_space_not_literal_nbsp() {
    // A GENERATED non-breaking space (an escaped space `\ ` or line-block
    // indent) must render as an ASCII space in plain/ANSI, while a LITERAL
    // U+00A0 typed in the source (test above) is preserved. Only HTML/Markdown
    // fold the escaped space back to `&nbsp;` / a literal NBSP.
    assert_eq!(carve::to_plain_text("10\\ kg").as_bytes(), b"10 kg\n");
    assert_eq!(carve::to_ansi("10\\ kg").as_bytes(), b"10 kg\n");
    assert!(carve::to_html("10\\ kg").contains("10&nbsp;kg"));
    assert_eq!(carve::to_markdown("10\\ kg").as_bytes(), b"10\xc2\xa0kg\n");

    // Line-block leading indentation is generated-NBSP too: ASCII spaces in
    // plain output, not literal U+00A0.
    let verse = "::: |\n  indented\nflush\n:::\n";
    assert!(carve::to_plain_text(verse).starts_with("  indented"));
}

#[test]
fn ansi_table_header_code_keeps_nested_code_color() {
    let out = carve::to_ansi("| `a|b` | c |\n|--|--|\n| d | e |");

    assert!(out.contains("\x1b[1m\x1b[93ma|b\x1b[0m\x1b[0m"), "{out:?}");
    assert!(out.contains("\x1b[1mc\x1b[0m"), "{out:?}");
}

#[test]
fn markdown_admonition_title_unwraps_nested_strong_only_in_title() {
    assert_eq!(
        carve::to_markdown("::: note \"a *b* `c`\"\nx\n:::"),
        "**a b `c`**\n\nx\n"
    );
    assert_eq!(
        carve::to_markdown("::: note \"a /em/ d\"\nx\n:::"),
        "**a *em* d**\n\nx\n"
    );
    assert_eq!(
        carve::to_markdown("::: note \"t\"\n*a*\n:::"),
        "**t**\n\n**a**\n"
    );
}

#[test]
fn ansi_admonition_title_unwraps_nested_strong_only_in_title() {
    assert_eq!(
        carve::to_ansi("::: note \"a *b* `c`\"\nx\n:::"),
        "\x1b[1ma b \x1b[93mc\x1b[0m\x1b[0m\n\nx\n"
    );
    assert_eq!(
        carve::to_ansi("::: note \"a /em/ d\"\nx\n:::"),
        "\x1b[1ma \x1b[3mem\x1b[0m d\x1b[0m\n\nx\n"
    );
    assert_eq!(
        carve::to_ansi("::: note \"t\"\n*a*\n:::"),
        "\x1b[1mt\x1b[0m\n\n\x1b[1ma\x1b[0m\n"
    );
}

use super::*;

#[test]
fn lowercase_same_file_heading_fragment_resolves() {
    assert!(
        markdown_to_carve("# Install Steps\n\n[steps](#install-steps)")
            .contains("[steps](#Install-Steps)")
    );
    assert!(
        markdown_to_carve("# Install Steps\n\n# install steps\n\n[steps](#INSTALL-STEPS)")
            .contains("[steps](#INSTALL-STEPS)")
    );
    assert!(markdown_to_carve(
        "# Install Steps\n\n<a id=\"install-steps\"></a>\n\n[steps](#install-steps)"
    )
    .contains("[steps](#install-steps)"));
    assert!(
        markdown_to_carve("# Foo\n\n# Foo\n\n# Foo\n\n[third](#foo-2)").contains("[third](#foo-2)")
    );
    assert!(markdown_to_carve("> # Quote Head\n\n[quote](#quote-head)")
        .contains("[quote](#Quote-Head)"));
    assert!(
        markdown_to_carve("# Real\n\n<h1 id=\"Fake\">Raw</h1>\n\n[raw](#fake)")
            .contains("[raw](#fake)")
    );
    assert!(
        markdown_to_carve("# Fake\n\n<h1 id=\"Fake\">Raw</h1>\n\n[native](#fake)")
            .contains("[native](#Fake)")
    );
    assert!(markdown_to_carve("# Version 02\n\n[version](#version-02)")
        .contains("[version](#Version-02)"));
    assert!(markdown_to_carve("# ΟΔΟΣ\n\n[street](#οδος)").contains("#ΟΔΟΣ"));
}

/// markup-carve/carve#2799. A BARE `---` block becomes frontmatter only
/// when its first content line has the shape of a mapping.
#[test]
fn only_a_mapping_shaped_bare_block_becomes_frontmatter() {
    let cases: &[(&str, bool, &str)] = &[
        ("---\ntitle: Hi\n---\nB\n", true, "a real mapping"),
        ("---\n# just a note\n---\nB\n", false, "comment only"),
        ("---\n---\nB\n", false, "empty block"),
        (
            "---\n# note\ntitle: Hi\n---\nB\n",
            true,
            "comment above a key",
        ),
        ("---\n\"my key\": Hi\n---\nB\n", true, "double-quoted key"),
        ("---\n'my key': Hi\n---\nB\n", true, "single-quoted key"),
        ("---\nfoo:bar: Hi\n---\nB\n", false, "key holding a colon"),
        ("---\ntitle:Hi\n---\nB\n", false, "no space after the colon"),
        ("---\n- one\n---\nB\n", false, "a list, not a mapping"),
        (
            "---\ntitle: [unclosed\n---\nB\n",
            true,
            "malformed but mapping-shaped",
        ),
        (
            "---\nFoo\n---\nBar\n---\nBaz\n",
            false,
            "CommonMark example 96",
        ),
        ("---\n  title: Hi\n---\nB\n", false, "key not at column 0"),
        ("---\ntitle:\n---\nB\n", true, "colon at end of line"),
        (
            "---\n{a: 1}\n---\nB\n",
            false,
            "a flow mapping is not a key line",
        ),
        ("---\n: Hi\n---\nB\n", false, "an empty key is no key"),
    ];
    for (markdown, expected, label) in cases {
        let document = markdown_to_ast(markdown);
        assert_eq!(
            document.frontmatter_raw.is_some(),
            *expected,
            "{label}: {markdown:?} came out {:?}",
            document.frontmatter_raw
        );
    }
}

/// carve-rs#2388. A typed opener was read as paragraph text, with the
/// closer below acting as a setext underline, so the payload landed in an
/// `h2`. It is detected now - and never shape-tested, because `---FORMAT`
/// says what the block is (markup-carve/carve#2799).
#[test]
fn a_typed_opener_is_frontmatter_whatever_its_content() {
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "---toml\n[table]\ntitle = \"Hi\"\n---\n\nBody.\n",
            "toml",
            "[table]\ntitle = \"Hi\"",
            "a table header",
        ),
        (
            "--- toml\ntitle = \"Hi\"\n---\n\nBody.\n",
            "toml",
            "title = \"Hi\"",
            "PART 1's lenient one-space form",
        ),
        (
            "---json\n{\"a\": 1}\n---\n\nBody.\n",
            "json",
            "{\"a\": 1}",
            "a json object, no shape rule needed",
        ),
        (
            "---yaml\nFoo\n---\n\nBody.\n",
            "yaml",
            "Foo",
            "a scalar payload is the author's business",
        ),
        (
            "---neon\n- one\n---\n\nBody.\n",
            "neon",
            "- one",
            "any frontmatter_format",
        ),
        (
            "---toml\n---\n\nBody.\n",
            "toml",
            "",
            "an empty typed block",
        ),
    ];
    for (markdown, format, content, label) in cases {
        let document = markdown_to_ast(markdown);
        let raw = document
            .frontmatter_raw
            .as_ref()
            .unwrap_or_else(|| panic!("{label}: {markdown:?} kept no frontmatter"));
        assert_eq!(&raw.format, format, "{label}");
        assert_eq!(&raw.content, content, "{label}");
    }
    // The lenient spelling canonicalizes to `---toml` on the way out.
    assert_eq!(
        markdown_to_carve("--- toml\ntitle = \"Hi\"\n---\n\nBody.\n"),
        "---toml\ntitle = \"Hi\"\n---\n\nBody.\n"
    );
}

/// A claimed typed block leaves pulldown's metadata path off, so a `---`
/// opening the BODY cannot be read as a second frontmatter block.
#[test]
fn a_claimed_typed_block_does_not_let_the_body_open_another() {
    let document = markdown_to_ast("---toml\nkey = 1\n---\n---\ntitle: Hi\n---\n");
    let raw = document.frontmatter_raw.as_ref().expect("the typed block");
    assert_eq!(raw.format, "toml");
    assert_eq!(raw.content, "key = 1");
}

/// One triple across all three engines (markup-carve/carve#2806):
/// `normalized` because an alternate block form was resolved, `line:1`
/// because a synthesized block starts there, and a confidence that follows
/// the opener. A typed opener declared its format, so nothing is inferred.
#[test]
fn the_synthesized_row_carries_one_triple() {
    for (source, confidence) in [
        (
            "---\ntitle: x\n---\n\n# Heading\n\nText here.\n",
            crate::ImportConfidence::Inferred,
        ),
        (
            "---yaml\ntitle: x\n---\n\n# Heading\n\nText here.\n",
            crate::ImportConfidence::Exact,
        ),
    ] {
        let report = crate::migrate_markdown(source).report;
        let rows: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == "frontmatter-synthesized")
            .collect();
        assert_eq!(rows.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(rows[0].severity, crate::HtmlImportSeverity::Info);
        assert_eq!(rows[0].fidelity, crate::ImportFidelity::Normalized);
        assert_eq!(rows[0].confidence, confidence, "{source:?}");
        assert_eq!(rows[0].path.as_deref(), Some("line:1"));
    }
}

/// CommonMark example 96: a thematic break, two setext `h2`s, a paragraph.
/// The break comes out in a spelling that cannot reopen frontmatter.
#[test]
fn example_96_keeps_its_commonmark_meaning() {
    assert_eq!(
        markdown_to_carve("---\nFoo\n---\nBar\n---\nBaz\n"),
        "---\n\n## Foo\n\n## Bar\n\nBaz\n"
    );
    // The writer's own frontmatter-safe fallback covers the shapes where a
    // byte-0 `---` could gain a closer from a later break.
    assert_eq!(markdown_to_carve("---\n---\nB\n"), "***\n\n***\n\nB\n");
    assert!(!crate::parse::opens_frontmatter(&markdown_to_carve(
        "---\n# just a note\n---\nB\n"
    )));
}

#[test]
fn every_frontmatter_conversion_is_reported() {
    let report = crate::migrate_markdown("---\ntitle: Hi\n---\nB\n").report;
    let synthesized: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "frontmatter-synthesized")
        .collect();
    assert_eq!(synthesized.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(synthesized[0].fidelity, crate::ImportFidelity::Normalized);
    // The typed path reports too, with its own reason.
    let typed = crate::migrate_markdown("---toml\nkey = 1\n---\n\nB\n").report;
    assert_eq!(
        typed
            .diagnostics
            .iter()
            .filter(|d| d.code == "frontmatter-synthesized")
            .count(),
        1,
        "{:?}",
        typed.diagnostics
    );
    let declined = crate::migrate_markdown("---\nFoo\n---\nBar\n---\nBaz\n").report;
    assert!(declined
        .diagnostics
        .iter()
        .all(|d| d.code != "frontmatter-synthesized"));
}

#[test]
fn markdown_literal_crossrefs_stay_text() {
    for source in [
        "# id\n\n&lt;/#id>\n",
        "# id\n\n**&lt;/#id>**\n",
        "# id\n\n&#60;/#id>\n",
        "# id\n\n&lt;&#47;#id>\n",
        "# id\n\n&lt;/&#35;id>\n",
        "# id\n\n&lt;\\/#id>\n",
        "# id\n\n&lt;/#id> https://x.com\n",
    ] {
        let written = markdown_to_carve(source);
        let html = crate::to_html(&written);
        assert!(
            !html.contains("href=\"#id\""),
            "{source:?} became {written:?}"
        );
        assert!(
            html.contains("&lt;/#id&gt;"),
            "{source:?} became {written:?}"
        );
    }
}

#[test]
fn html_code_entity_stays_native_code() {
    let source = "Use <code>&lt;div&gt;</code> here.\n";
    let written = markdown_to_carve(source);
    assert_eq!(written, "Use `<div>` here.\n");
}

#[test]
fn emphasis_takes_the_carve_spelling() {
    assert_eq!(markdown_to_carve("*em*"), "/em/\n");
    assert_eq!(markdown_to_carve("**strong**"), "*strong*\n");
    assert_eq!(markdown_to_carve("_em_"), "/em/\n");
}

#[test]
fn nested_emphasis_keeps_both_families() {
    assert_eq!(markdown_to_carve("*a **b** c*"), "/a *b* c/\n");
}

#[test]
fn headings_keep_their_level() {
    assert_eq!(markdown_to_carve("# One"), "# One\n");
    assert_eq!(markdown_to_carve("### Three"), "### Three\n");
}

#[test]
fn a_setext_heading_becomes_an_atx_one() {
    // The tree records a level-1 heading; the spelling is the writer's.
    assert_eq!(markdown_to_carve("Title\n====="), "# Title\n");
}

#[test]
fn a_fence_keeps_its_language_and_body() {
    // The Markdown source is the lenient spelling and the output is the
    // canonical one: `fenced_code_block` names the no-space form canonical
    // and says it is what the X->Carve converters emit, and this importer
    // ends at the canonical writer.
    assert_eq!(
        markdown_to_carve("``` js\nlet x = 1\n```"),
        "```js\nlet x = 1\n```\n"
    );
    assert_eq!(
        markdown_to_carve("```js\nlet x = 1\n```"),
        "```js\nlet x = 1\n```\n"
    );
}

#[test]
fn an_info_string_contributes_only_its_first_word() {
    assert!(markdown_to_carve("``` js title=x\ny\n```").starts_with("```js\n"));
}

#[test]
fn a_code_span_is_verbatim() {
    assert_eq!(markdown_to_carve("`*not em*`"), "`*not em*`\n");
}

#[test]
fn lists_survive_with_their_nesting() {
    assert_eq!(markdown_to_carve("- a\n- b"), "- a\n- b\n");
    assert!(markdown_to_carve("- a\n  - b").contains("- b"));
    assert_eq!(markdown_to_carve("1. a\n2. b"), "1. a\n2. b\n");
}

#[test]
fn a_changed_markdown_bullet_remains_a_second_list() {
    let value = markdown_to_carve("- x\n+ y\n");
    assert_eq!(value, "- x\n\n* y\n");
    assert_eq!(
        crate::to_html(&value),
        "<ul>\n  <li>x</li>\n</ul>\n<ul>\n  <li>y</li>\n</ul>"
    );
    let value = markdown_to_carve("* x\n+ y\n");
    assert_eq!(
        crate::to_html(&value),
        "<ul>\n  <li>x</li>\n</ul>\n<ul>\n  <li>y</li>\n</ul>"
    );
}

#[test]
fn a_task_list_keeps_its_checkbox() {
    let out = markdown_to_carve("- [x] done\n- [ ] todo");
    assert!(out.contains("[x]"), "{out}");
    assert!(out.contains("[ ]"), "{out}");
}

#[test]
fn a_blockquote_survives() {
    assert_eq!(markdown_to_carve("> quoted"), "> quoted\n");
}

#[test]
fn a_link_and_an_image_survive() {
    assert_eq!(
        markdown_to_carve("[text](https://e.com)"),
        "[text](https://e.com)\n"
    );
    assert_eq!(markdown_to_carve("![alt](a.png)"), "![alt](a.png)\n");
}

#[test]
fn a_gfm_table_becomes_a_carve_table() {
    let out = markdown_to_carve("| A | B |\n|---|---|\n| 1 | 2 |");
    assert!(out.contains('A') && out.contains('B'), "{out}");
    assert!(out.contains("| 1 | 2 |"), "{out}");
}

#[test]
fn strikethrough_takes_the_single_delimiter() {
    assert_eq!(markdown_to_carve("~~gone~~"), "~gone~\n");
}

#[test]
fn a_thematic_break_survives() {
    assert_eq!(markdown_to_carve("---"), "---\n");
}

#[test]
fn an_html_block_becomes_a_raw_block() {
    assert_eq!(
        markdown_to_carve("<div>\nraw\n</div>"),
        "```=html\n<div>\nraw\n</div>\n```\n"
    );
}

#[test]
fn consecutive_html_lines_are_one_raw_block() {
    // The parser hands block HTML a chunk at a time; two raw blocks here
    // would be two `=html` fences where the author wrote one element.
    assert_eq!(
        markdown_to_carve("<div>\nraw\n</div>")
            .matches("```")
            .count(),
        2
    );
}

#[test]
fn inline_html_uses_native_nodes_or_raw_html() {
    assert_eq!(markdown_to_carve("a <b>c</b> d"), "a *c* d\n");
    assert_eq!(
        markdown_to_carve("a <span>c</span> d"),
        "a `<span>`{=html}c`</span>`{=html} d\n"
    );
    assert_eq!(markdown_to_carve("use <code>x=1</code>"), "use `x=1`\n");
    assert_eq!(
        markdown_to_carve("before <!-- c --> after"),
        "before `<!-- c -->`{=html} after\n"
    );
}

#[test]
fn html_highlight_uses_contextual_bracing() {
    assert_eq!(markdown_to_carve("a <mark>x</mark> b"), "a =x= b\n");
    assert_eq!(markdown_to_carve("a<mark>x</mark>b"), "a{=x=}b\n");
}

#[test]
fn frontmatter_survives_instead_of_becoming_a_heading() {
    let document = markdown_to_ast("---\ntitle: T\n---\n\nBody.");
    assert_eq!(
        document.frontmatter.get("title").map(String::as_str),
        Some("T")
    );
    assert_eq!(document.children.len(), 1);
}

#[test]
fn a_footnote_keeps_its_definition_instead_of_becoming_a_link() {
    let document = markdown_to_ast("Text[^1]\n\n[^1]: Note.");
    assert!(document.footnote_defs.contains_key("1"));
    assert_eq!(
        markdown_to_carve("Text[^1]\n\n[^1]: Note."),
        "Text[^1]\n\n[^1]: Note.\n"
    );
}

#[test]
fn a_loose_list_stays_loose() {
    // Looseness is what puts a `<p>` inside each `<li>`, so collapsing it
    // changes the rendered document, not only the source.
    assert_eq!(markdown_to_carve("- a\n\n- b"), "- a\n\n- b\n");
    assert_eq!(markdown_to_carve("- a\n- b"), "- a\n- b\n");
}

#[test]
fn an_empty_document_stays_empty() {
    assert_eq!(markdown_to_carve("").trim(), "");
}

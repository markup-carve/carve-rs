//! carve#6538: `braced-span-opener-as-text` takes a braced closer of the opener's own mark, later in
//! the same leaf block. carve-rs#4706: the finding names the text node, and footnote tokens are
//! reported by the node rules.

use carve::lint::{LintOptions, lint};

fn findings(source: &str, code: &str, options: &LintOptions) -> Vec<(u32, u32, String)> {
    lint(source, options)
        .into_iter()
        .filter(|finding| finding.code == code)
        .map(|finding| {
            let pos = finding.pos.expect("pos");
            (
                pos.start_offset,
                pos.end_offset,
                finding.path.unwrap_or_default(),
            )
        })
        .collect()
}

fn openers(source: &str) -> Vec<(u32, u32)> {
    findings(
        source,
        "braced-span-opener-as-text",
        &LintOptions::default(),
    )
    .into_iter()
    .map(|(from, to, _)| (from, to))
    .collect()
}

fn github() -> LintOptions {
    let mut options = LintOptions::default();
    options.platforms.push("github".to_string());
    options
}

#[test]
fn ticket_table_rows() {
    let rows: [(&str, &[(u32, u32)]); 8] = [
        ("*a {/b*/}", &[(3, 5)]),
        ("*a {/b* c\n\n*bold {/note*/} more*", &[(17, 19)]),
        ("*a {/b* c\nd /} e", &[(3, 5)]),
        ("*a {/b* c *}", &[]),
        ("*a {/b* c }", &[]),
        ("/a {*b/*}", &[(3, 5)]),
        ("> *a {/b*\n> /}", &[(5, 7)]),
        ("*a {/b* c\n\nd /}", &[]),
    ];
    for (source, expected) in rows {
        assert_eq!(openers(source), expected, "{source:?}");
    }
}

#[test]
fn pairing_rows() {
    let rows: [(&str, &[(u32, u32)]); 18] = [
        // Every opener gets its own finding.
        ("*a {/b {=c* d=} e/}", &[(3, 5), (7, 9)]),
        // C2-I-058, C2-I-059: the nearest open opener of the mark takes the closer.
        ("*a {/b* c {/d/}", &[]),
        ("*a {/b {/c* d/}", &[(7, 9)]),
        ("*a {/b {=c* d/} e=}", &[(3, 5)]),
        ("*a {=b* c=}", &[(3, 5)]),
        ("{+a *b {/c* d +} e /}", &[]),
        ("{+a *b {/c* d /} e +}", &[(7, 9)]),
        // Escaped, in code, in math, in an attribute value: no closer.
        ("*a \\{/b* /}", &[]),
        ("*a {/b* \\/}", &[]),
        ("*a {/b* `/}`", &[]),
        ("*a {/b* $`/}`", &[]),
        ("*a {/b* c {#id title=\"/}\"}", &[]),
        ("*a {_key=v} b _}*", &[]),
        // C2-I-007: a closer pairs only inside its bracket.
        ("*a {/b* ^[x /}]", &[]),
        ("[*a {/b* c](u) d /}", &[]),
        ("[*a {/b* c /}](u)", &[(4, 6)]),
        ("*a {/b* [x](u) c /}", &[(3, 5)]),
        ("```\nhi\n```\n^ *a {/b* c /}", &[(16, 18)]),
    ];
    for (source, expected) in rows {
        assert_eq!(openers(source), expected, "{source:?}");
    }
}

#[test]
fn the_path_names_the_text_node() {
    assert_eq!(
        findings(
            "*a {/b*/}",
            "braced-span-opener-as-text",
            &LintOptions::default()
        ),
        [(3, 5, "/children/0/children/0/children/0".to_string())]
    );
    assert_eq!(
        findings(
            "> *a {/b*\n> /}",
            "braced-span-opener-as-text",
            &LintOptions::default()
        ),
        [(
            5,
            7,
            "/children/0/children/0/children/0/children/0".to_string()
        )]
    );
}

const MARKS: [char; 9] = ['*', '/', '_', '~', '=', '+', '-', '^', ','];

type Placement = fn(char, char, char) -> String;

/// Each placement builds the source for bare parent `p`, opener mark `m` and another mark `w`.
const PLACEMENTS: [(&str, Placement, bool); 13] = [
    (
        "same line",
        |p, m, _| format!("{p}a {{{m}b{p} c {m}}}"),
        true,
    ),
    (
        "later line",
        |p, m, _| format!("{p}a {{{m}b{p} c\nd {m}}} e"),
        true,
    ),
    (
        "next paragraph",
        |p, m, _| format!("{p}a {{{m}b{p} c\n\nd {m}}}"),
        false,
    ),
    (
        "quote lines",
        |p, m, _| format!("> {p}a {{{m}b{p}\n> {m}}}"),
        true,
    ),
    (
        "list item lines",
        |p, m, _| format!("- {p}a {{{m}b{p} c\n  d {m}}}"),
        true,
    ),
    (
        "list item paragraphs",
        |p, m, _| format!("- {p}a {{{m}b{p} c\n\n  d {m}}}"),
        false,
    ),
    (
        "other mark",
        |p, m, w| format!("{p}a {{{m}b{p} c {w}}}"),
        false,
    ),
    (
        "escaped closer",
        |p, m, _| format!("{p}a {{{m}b{p} c \\{m}}}"),
        false,
    ),
    (
        "closer in code",
        |p, m, _| format!("{p}a {{{m}b{p} c `{m}}}`"),
        false,
    ),
    ("no closer", |p, m, _| format!("{p}a {{{m}b{p} c"), false),
    (
        "table cell",
        |p, m, _| format!("| {p}a {{{m}b{p} c {m}}} |"),
        true,
    ),
    (
        "heading",
        |p, m, _| format!("# {p}a {{{m}b{p} c {m}}}"),
        true,
    ),
    (
        "quote paragraphs",
        |p, m, _| format!("> {p}a {{{m}b{p}\n>\n> {m}}}"),
        false,
    ),
];

#[test]
fn generated_variants() {
    let mut count = 0;
    for parent in ['*', '/'] {
        for mark in MARKS {
            let other = if mark == '/' { '+' } else { '/' };
            for (name, build, reports) in PLACEMENTS {
                let source = build(parent, mark, other);
                let at = u32::try_from(source.find('{').expect("opener")).expect("offset");
                let expected = if reports { vec![(at, at + 2)] } else { vec![] };
                assert_eq!(
                    openers(&source),
                    expected,
                    "{parent} {mark} {name}: {source:?}"
                );
                count += 1;
            }
        }
    }
    assert!(count >= 200, "{count}");
}

/// Source, code and the expected range and path of each finding.
type FootnoteRow = (
    &'static str,
    &'static str,
    &'static [(u32, u32, &'static str)],
);

#[test]
fn referenced_footnote_tokens_carry_their_node_path() {
    let rows: [FootnoteRow; 6] = [
        (
            "[^n]: #12\n\n[^n]\n",
            "platform-issue-reference",
            &[(6, 9, "/children/0/children/0/children/0")],
        ),
        (
            "[^n]: see #12\n\n[^n]\n",
            "platform-issue-reference",
            &[(10, 13, "/children/0/children/0/children/0")],
        ),
        (
            "[^n]: a\n  #12\n\n[^n]\n",
            "platform-issue-reference",
            &[(10, 13, "/children/0/children/0/children/2")],
        ),
        ("[^n]: #12\n", "platform-issue-reference", &[]),
        (
            "[^n]: see @bob\n\n[^n]\n",
            "platform-mention-token",
            &[(10, 14, "/children/0/children/0/children/1")],
        ),
        (
            "[^n]: @bob\n\n[^n]\n",
            "platform-mention-token",
            &[(6, 10, "/children/0/children/0/children/0")],
        ),
    ];
    for (source, code, expected) in rows {
        let expected: Vec<_> = expected
            .iter()
            .map(|(from, to, path)| (*from, *to, (*path).to_string()))
            .collect();
        assert_eq!(findings(source, code, &github()), expected, "{source:?}");
    }
}

#[test]
fn unreferenced_and_opaque_footnote_tokens_stay_quiet() {
    for source in [
        "[^n]: a\n\n  b #3 @al\n",
        "```\n[^n]: #12\n```\n\n[^n]\n",
        "[^m]\n\n[^n]: #1\n",
    ] {
        let reported: Vec<_> = lint(source, &github())
            .into_iter()
            .filter(|finding| finding.code.starts_with("platform-"))
            .collect();
        assert!(reported.is_empty(), "{source:?}: {reported:?}");
    }
}

#[test]
fn a_leaf_that_starts_with_a_multibyte_character() {
    for source in ["é *a {/b* c /}", "日本 /ä {*ö/ ü\nd *} e"] {
        let at = u32::try_from(source.find('{').expect("opener")).expect("offset");
        assert_eq!(openers(source), [(at, at + 2)], "{source:?}");
    }
}

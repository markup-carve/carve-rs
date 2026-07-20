use std::fs;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus")
}

fn corpus_sources() -> Vec<(String, String)> {
    let dir = corpus_dir();
    if !dir.exists() {
        panic!(
            "Spec corpus not found at {}.\n\
             Did you initialize the submodule?\n  git submodule update --init",
            dir.display()
        );
    }
    let mut out: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("crv") {
                return None;
            }
            let slug = path.file_stem()?.to_str()?.to_string();
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            Some((slug, source))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn corpus_formatter_semantic_idempotent_and_reparseable() {
    for (slug, source) in corpus_sources() {
        let formatted = carve::to_carve(&source);
        if !formatted.contains("\n\n    -")
            && !formatted.contains("\n\n     -")
            && !formatted.contains("\n\n     1.")
            && slug != "103-marker-line-nested-lists-2"
            && slug != "115-footnote-definition-inside-a-container-is-collected-2"
        {
            assert_eq!(
                carve::to_html(&formatted),
                carve::to_html(&source),
                "formatted corpus source changed HTML semantics for {slug}"
            );
        }
        if slug != "103-marker-line-nested-lists-2" {
            assert_eq!(
                carve::to_carve(&formatted),
                formatted,
                "formatted corpus source is not idempotent for {slug}"
            );
        }
        let _ = carve::parse(&formatted);
    }
}

#[test]
fn blank_line_collapse() {
    assert_eq!(carve::to_carve("a\n\n\n\nb\n"), "a\n\nb\n");
}

#[test]
fn bullet_marker_normalization() {
    let doc = carve::Document {
        frontmatter: Default::default(),
        footnote_defs: Default::default(),
        children: vec![carve::BlockNode::List(carve::List {
            attrs: None,
            ordered: false,
            start: None,
            ol_type: None,
            delim: None,
            bullet_char: None,
            tight: true,
            items: vec![carve::ListItem {
                attrs: None,
                checked: None,
                children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
                    attrs: None,
                    children: vec![carve::InlineNode::Text("a".to_string())],
                })],
            }],
        })],
        source_len: 0,
    };
    assert_eq!(carve::render_carve(&doc), "- a\n");
}

#[test]
fn fence_sizing_with_inner_backticks() {
    let source = "```\na ``` fence\n```\n";
    assert_eq!(carve::to_carve(source), "````\na ``` fence\n````\n");
}

#[test]
fn attribute_source_order_is_preserved() {
    assert_eq!(
        carve::to_carve("{k=v .cls #id}\n# H\n"),
        "{k=v .cls #id}\n# H\n"
    );
}

#[test]
fn strips_trailing_whitespace_but_preserves_nbsp() {
    assert_eq!(carve::to_carve("a  \n\u{00a0}  \n"), "a\n\u{00a0}\n");
}

#[test]
fn generic_line_block_div_keeps_soft_breaks() {
    let formatted = carve::to_carve("{.line-block}\n:::\na\nb\n:::\n");
    assert_eq!(formatted, "{.line-block}\n:::\na\nb\n:::\n");
    assert!(!formatted.contains("::: |"));
}

#[test]
fn inline_delimiter_emission() {
    assert_eq!(
        carve::to_carve("/i/ *b* _u_ ~s~ {^sup^} {,sub,} =mark= `code`\n"),
        "/i/ *b* _u_ ~s~ {^sup^} {,sub,} =mark= `code`\n"
    );
}

#[test]
fn literal_caret_escaped_literal_comma_unescaped() {
    // `^sup^` / `,sub,` are plain text (no bare sup/sub delimiter): the comma
    // needs no escape; the caret keeps one (footnote/caption channels).
    assert_eq!(
        carve::to_carve("^sup^ ,sub, stays literal\n"),
        "\\^sup\\^ ,sub, stays literal\n"
    );
}

// Verbatim content survives document normalization (carve-js issue 340):
// trailing whitespace and blank-line runs inside code blocks, raw blocks,
// frontmatter, and block comments are byte-exact after fmt.
#[test]
fn verbatim_content_survives_normalization() {
    for src in [
        "```\ntrailing   \nalso\t\t\n```\n",
        "```\na\n\n\n\nb\n```\n",
        "```=html\n<pre>x   \n\n\n\ny</pre>\n```\n",
        "%%%\nc   \n\n\n\nd\n%%%\n\nbody\n",
    ] {
        let formatted = carve::to_carve(src);
        assert_eq!(formatted, src);
        assert_eq!(carve::to_html(&formatted), carve::to_html(src));
    }
}

#[test]
fn verbatim_content_stable_inside_containers() {
    for src in [
        "> ```\n> a   \n>\n>\n>\n> b\n> ```\n",
        "- item\n\n  ```\n  a   \n\n\n\n  b\n  ```\n",
    ] {
        let f1 = carve::to_carve(src);
        let f2 = carve::to_carve(&f1);
        assert_eq!(f1, f2);
        assert_eq!(carve::to_html(&f1), carve::to_html(src));
    }
}

// The list marker is semantic (§11): a sibling with a different bullet char
// or ordered delimiter starts a NEW list, so fmt preserves the authored
// marker (carve issue 286) - normalizing would merge adjacent sibling lists.
#[test]
fn preserves_authored_list_markers() {
    for src in [
        "1) a\n2) b\n",
        "1. a\n2. b\n",
        "* a\n* b\n",
        "- a\n- b\n",
        "* [x] done\n* [ ] todo\n",
    ] {
        assert_eq!(carve::to_carve(src), src);
    }
}

#[test]
fn adjacent_lists_separated_by_marker_stay_separate() {
    // fmt invariant: to_html(fmt(x)) == to_html(x). Before marker
    // preservation these merged into one list on re-parse.
    for src in ["1. a\n1) b", "1. a\n\n1) b", "- a\n* b", "- a\n\n* b"] {
        let f1 = carve::to_carve(src);
        assert_eq!(carve::to_carve(&f1), f1);
        assert_eq!(carve::to_html(&f1), carve::to_html(src));
    }
}

// ---------------------------------------------------------------------------
// I1: `carve fmt` preserves include directives
// ---------------------------------------------------------------------------

/// The core has no directive node - `{{ path }}` is plain TEXT (I1) - so the
/// serializer used to escape it like any other punctuation-bearing text and
/// write `\{\{ path \}\}`.
///
/// The formatter's existing invariant, `to_html(fmt(x)) == to_html(x)`, cannot
/// see that: the escaped form renders as the very same literal text, so the
/// suite stayed green while every include in a formatted document was
/// destroyed. Nothing looks broken until a resolver runs and the chapters have
/// silently vanished.
///
/// These tests therefore assert the INTENT instead: EXPANDING the formatted
/// document must give the same result - same HTML and same dependency set - as
/// expanding the original.
mod include_directives_survive_formatting {
    use carve::{expand_includes, parse, render_html, IncludeOptions, IncludeResolved};

    const FILES: &[(&str, &str)] = &[
        ("chapter.crv", "# Chapter\n\nBody."),
        ("my chapter.crv", "Spaced body."),
        (
            "book.crv",
            "# A\n\nskip\n\n{#intro}\n# Intro\n\nIntro body.",
        ),
        ("lines.crv", "skip\nOne\nTwo\nskip"),
        ("shift.crv", "# Shifted"),
        ("inline.crv", "inline body"),
    ];

    struct Expansion {
        html: String,
        dependencies: Vec<(String, bool)>,
    }

    fn expand(source: &str) -> Expansion {
        let resolver = |path: &str, _ctx: &carve::IncludeContext<'_>| {
            FILES
                .iter()
                .find(|(name, _)| *name == path)
                .map(|(_, body)| IncludeResolved::from(*body))
        };
        let opts = IncludeOptions::new().with_resolver(&resolver);
        let result = expand_includes(parse(source), source, &opts);
        Expansion {
            html: render_html(&result.doc),
            dependencies: result
                .dependencies
                .into_iter()
                .map(|d| (d.id, d.resolved))
                .collect(),
        }
    }

    /// The real assertion: formatting must not change what the document
    /// INCLUDES, which the round-trip invariant could never express.
    fn assert_survives(source: &str) {
        let formatted = carve::to_carve(source);
        let before = expand(source);
        let after = expand(&formatted);
        assert_eq!(
            after.html, before.html,
            "formatting changed the expanded output of {source:?} (formatted: {formatted:?})"
        );
        assert_eq!(
            after.dependencies, before.dependencies,
            "formatting changed the dependency set of {source:?} (formatted: {formatted:?})"
        );
        assert_eq!(
            carve::to_carve(&formatted),
            formatted,
            "formatting {source:?} is not idempotent"
        );
    }

    #[test]
    fn bare_path_is_preserved() {
        assert_eq!(
            carve::to_carve("{{ chapter.crv }}\n"),
            "{{ chapter.crv }}\n"
        );
        assert_survives("{{ chapter.crv }}\n");
    }

    #[test]
    fn quoted_path_with_spaces_is_preserved() {
        // Also pins that the path is NOT run through smart typography, which
        // would curl the quotes into a path naming a different file.
        assert_eq!(
            carve::to_carve("{{ \"my chapter.crv\" }}\n"),
            "{{ \"my chapter.crv\" }}\n"
        );
        assert_survives("{{ \"my chapter.crv\" }}\n");
    }

    #[test]
    fn section_selection_is_preserved() {
        // `#intro` arrives as a Tag node, so recognition has to reassemble the
        // run exactly as expansion does (I9a).
        assert_eq!(
            carve::to_carve("{{ book.crv #intro }}\n"),
            "{{ book.crv #intro }}\n"
        );
        assert_survives("{{ book.crv #intro }}\n");
    }

    #[test]
    fn line_range_option_is_preserved() {
        assert_eq!(
            carve::to_carve("{{ lines.crv @lines:2-3 }}\n"),
            "{{ lines.crv @lines:2-3 }}\n"
        );
        assert_survives("{{ lines.crv @lines:2-3 }}\n");
    }

    #[test]
    fn shift_options_are_preserved() {
        // `@shift:2` arrives as a Mention node.
        assert_eq!(
            carve::to_carve("{{ shift.crv @shift:2 }}\n"),
            "{{ shift.crv @shift:2 }}\n"
        );
        assert_survives("{{ shift.crv @shift:2 }}\n");
        assert_eq!(
            carve::to_carve("{{ shift.crv @shift:auto }}\n"),
            "{{ shift.crv @shift:auto }}\n"
        );
        assert_survives("# Top\n\n{{ shift.crv @shift:auto }}\n");
    }

    #[test]
    fn inline_directive_within_a_sentence_is_preserved() {
        assert_eq!(
            carve::to_carve("See {{ inline.crv }} now.\n"),
            "See {{ inline.crv }} now\\.\n"
        );
        assert_survives("See {{ inline.crv }} now.\n");
    }

    #[test]
    fn directives_in_code_are_left_exactly_as_the_core_produces_them() {
        // I9: code is verbatim and the directive is inert there, so the
        // serializer must not treat it specially - the code paths already emit
        // their content raw, and that must not regress into unescaping.
        assert_eq!(
            carve::to_carve("`{{ chapter.crv }}`\n"),
            "`{{ chapter.crv }}`\n"
        );
        assert_eq!(
            carve::to_carve("```txt\n{{ chapter.crv }}\n```\n"),
            "``` txt\n{{ chapter.crv }}\n```\n"
        );
        // And they stay inert after formatting.
        assert_survives("`{{ chapter.crv }}`\n");
        assert_survives("```txt\n{{ chapter.crv }}\n```\n");
    }

    #[test]
    fn malformed_runs_are_still_escaped_as_ordinary_text() {
        // Not directive-shaped: no closing `}}`.
        assert_eq!(carve::to_carve("{{ oops\n"), "\\{\\{ oops\n");
        // Shaped, but the option tail is not valid per I1, so it is not a
        // well-formed directive and stays ordinary text.
        assert_eq!(
            carve::to_carve("{{ chapter.crv @bogus:1 }}\n"),
            "\\{\\{ chapter\\.crv @bogus\\:1 \\}\\}\n"
        );
        assert_survives("{{ oops\n");
    }

    #[test]
    fn preserved_directives_are_idempotent_under_repeated_formatting() {
        for source in [
            "{{ chapter.crv }}\n",
            "{{ \"my chapter.crv\" }}\n",
            "{{ book.crv #intro }}\n",
            "{{ lines.crv @lines:2-3 }}\n",
            "{{ shift.crv @shift:auto }}\n",
            "See {{ inline.crv }} now.\n",
        ] {
            let once = carve::to_carve(source);
            assert_eq!(carve::to_carve(&once), once, "not idempotent: {source:?}");
        }
    }
}

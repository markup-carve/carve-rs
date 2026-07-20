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
        // Two addressable sections, for directives that must BOTH survive in
        // one run while selecting different parts of the same file.
        (
            "two.crv",
            "{#intro}\n# Intro\n\nIntro body.\n\n{#outro}\n# Outro\n\nOutro body.",
        ),
        ("x.crv", "XXX"),
        ("y.crv", "YYY"),
        ("z.crv", "ZZZ"),
    ];

    struct Expansion {
        html: String,
        dependencies: Vec<(String, bool)>,
        rules: Vec<String>,
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
                .iter()
                .map(|d| (d.id.clone(), d.resolved))
                .collect(),
            rules: result.warnings.iter().map(|w| w.rule.clone()).collect(),
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
        // A directive that is preserved but whose DIAGNOSTICS are lost is
        // still a regression: the warning is the only thing that tells the
        // author their typo is a typo.
        assert_eq!(
            after.rules, before.rules,
            "formatting changed the include warnings of {source:?} (formatted: {formatted:?})"
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
    fn runs_that_are_not_shape_well_formed_are_still_escaped_as_ordinary_text() {
        // No closing `}}`.
        assert_eq!(carve::to_carve("{{ oops\n"), "\\{\\{ oops\n");
        // Closes, but carries no path token at all.
        assert_eq!(carve::to_carve("{{ }}\n"), "\\{\\{ \\}\\}\n");
        // A section or options but no path is likewise not a directive.
        assert_eq!(carve::to_carve("{{ #intro }}\n"), "\\{\\{ #intro \\}\\}\n");
        assert_eq!(
            carve::to_carve("{{ @lines:2-4 }}\n"),
            "\\{\\{ @lines\\:2\\-4 \\}\\}\n"
        );
        // The quoted form can spell an empty or whitespace-only path where the
        // bare form cannot; neither is a directive. Their quotes get CURLED,
        // which is itself the proof that they took the ordinary-text path: the
        // directive branch bypasses smart typography precisely so a real
        // quoted path is never curled into a name for a different file.
        assert_eq!(
            carve::to_carve("{{ \"\" }}\n"),
            "\\{\\{ \u{201c}\u{201c} \\}\\}\n"
        );
        assert_eq!(
            carve::to_carve("{{ \"   \" }}\n"),
            "\\{\\{ \u{201c}   \u{201c} \\}\\}\n"
        );
        assert_survives("{{ oops\n");
        assert_survives("{{ }}\n");
        assert_survives("{{ #intro }}\n");
        assert_survives("{{ @lines:2-4 }}\n");
        assert_survives("{{ \"\" }}\n");
        assert_survives("{{ \"   \" }}\n");
        // The empty token between two real directives is escaped without
        // taking its neighbors: both of those still expand.
        assert_survives("a {{ x.crv }} b {{ \"\" }} c {{ y.crv }} d\n");
    }

    /// Preservation is scoped to SHAPE, not validity (spec I1).
    ///
    /// `@bogus:1` is an invalid option, but the run is unmistakably a
    /// directive: it opens, closes, and names a path. Escaping it would freeze
    /// a one-character typo into permanent literal text AND silently drop the
    /// `include-unknown-option` warning that names the mistake - turning a
    /// fixable error into prose that merely looks like an error. Option
    /// validity is an expansion-time DIAGNOSTIC, never a preservation gate.
    #[test]
    fn a_shaped_directive_with_an_invalid_option_is_preserved_with_its_warning() {
        assert_eq!(
            carve::to_carve("{{ chapter.crv @bogus:1 }}\n"),
            "{{ chapter.crv @bogus:1 }}\n"
        );
        assert_eq!(
            expand("{{ chapter.crv @bogus:1 }}\n").rules,
            ["include-unknown-option"]
        );
        assert_survives("{{ chapter.crv @bogus:1 }}\n");
    }

    /// Same rule for the other selector: a `#section` the target does not have
    /// is a diagnostic (`include-section`), so the directive round-trips and
    /// the warning still fires on the formatted document.
    #[test]
    fn a_shaped_directive_with_a_missing_section_is_preserved_with_its_warning() {
        assert_eq!(
            carve::to_carve("{{ book.crv #nope }}\n"),
            "{{ book.crv #nope }}\n"
        );
        assert_eq!(expand("{{ book.crv #nope }}\n").rules, ["include-section"]);
        assert_survives("{{ book.crv #nope }}\n");
    }

    // -----------------------------------------------------------------------
    // More than ONE directive per run
    // -----------------------------------------------------------------------

    /// Every fixture above uses a SINGLE directive, which is exactly how this
    /// bug class stays invisible: an implementation that preserves the first
    /// directive and escapes the rest of the run passes all of them. carve-php
    /// had precisely that defect - it escaped the remainder wholesale instead
    /// of rescanning it - so `a {{ x }} b {{ y }} c` silently lost `{{ y }}`
    /// with two FULLY VALID directives.
    ///
    /// rs rescans (`split_run_directives` advances a cursor past each match and
    /// keeps looking), so it is correct here. These pin that property rather
    /// than trusting the loop to stay that way. Verified to be load-bearing:
    /// with the rescan disabled all four tests below fail while every
    /// single-directive test above still passes.
    #[test]
    fn every_directive_in_a_run_is_preserved_not_just_the_first() {
        for source in [
            // Two, three, four - prose between each.
            "a {{ x.crv }} b {{ y.crv }} c\n",
            "a {{ x.crv }} b {{ y.crv }} c {{ z.crv }} d\n",
            "{{ x.crv }} a {{ y.crv }} b {{ z.crv }} c {{ chapter.crv }}\n",
            // No prose between them at all.
            "{{ x.crv }}{{ y.crv }}\n",
            "{{ x.crv }} {{ y.crv }}\n",
            // First and last position in the run, nothing outside them.
            "{{ x.crv }} middle prose {{ y.crv }}\n",
        ] {
            assert_eq!(carve::to_carve(source), source, "not preserved: {source:?}");
            assert_survives(source);
        }
    }

    /// The highest-risk shape: `#section` parses to a Tag and `@option` to a
    /// Mention, so these runs are Text+Tag+Mention+Text sequences rather than
    /// one text node. A per-text-node scan finds none of them, and a scan that
    /// stops after the first match keeps only the leading one.
    #[test]
    fn multiple_sectioned_and_optioned_directives_all_survive_one_run() {
        for source in [
            "a {{ two.crv #intro }} b {{ two.crv #outro }} c\n",
            "a {{ lines.crv @lines:2-3 }} b {{ shift.crv @shift:2 }} c\n",
            "a {{ two.crv #intro }} b {{ shift.crv @shift:auto }} c {{ lines.crv @lines:1-2 }} d\n",
            "{{ two.crv #intro @shift:1 }} x {{ two.crv #outro @shift:2 }}\n",
        ] {
            assert_eq!(carve::to_carve(source), source, "not preserved: {source:?}");
            assert_survives(source);
        }
    }

    /// A run that mixes preservable and non-preservable directives must handle
    /// each on its own merits: the invalid one is escaped as ordinary text and
    /// the valid ones AROUND it - including the ones AFTER it - still survive.
    /// An implementation that bailed out of the run at the first non-match
    /// would silently drop everything downstream.
    #[test]
    fn an_unpreservable_directive_does_not_take_the_rest_of_the_run_with_it() {
        // Empty path in the middle: escaped, both neighbors intact.
        assert_eq!(
            carve::to_carve("a {{ x.crv }} b {{ }} c {{ y.crv }} d\n"),
            "a {{ x.crv }} b \\{\\{ \\}\\} c {{ y.crv }} d\n"
        );
        assert_survives("a {{ x.crv }} b {{ }} c {{ y.crv }} d\n");

        // Unclosed opener trailing a valid directive: only the opener escapes.
        assert_eq!(
            carve::to_carve("a {{ x.crv }} b {{ oops\n"),
            "a {{ x.crv }} b \\{\\{ oops\n"
        );
        assert_survives("a {{ x.crv }} b {{ oops\n");

        // A merely mis-OPTIONED directive is shape-well-formed, so all three
        // are preserved and the middle one keeps its warning.
        let mixed = "a {{ x.crv }} b {{ z.crv @bogus:1 }} c {{ y.crv }} d\n";
        assert_eq!(carve::to_carve(mixed), mixed);
        assert_eq!(expand(mixed).rules, ["include-unknown-option"]);
        assert_survives(mixed);
    }

    /// Directives spread across blocks, and across block vs inline position,
    /// are independent of each other.
    #[test]
    fn directives_survive_across_separate_blocks_and_mixed_positions() {
        for source in [
            "{{ x.crv }}\n\n{{ y.crv }}\n\n{{ z.crv }}\n",
            "{{ x.crv }}\n\nprose {{ y.crv }} prose\n",
            "prose {{ x.crv }} prose\n\n{{ y.crv }}\n",
            "- a {{ x.crv }} b {{ y.crv }} c\n",
            "> a {{ x.crv }} b {{ y.crv }} c\n",
            "# a {{ x.crv }} b {{ y.crv }}\n",
        ] {
            assert_eq!(carve::to_carve(source), source, "not preserved: {source:?}");
            assert_survives(source);
        }
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
            // Multi-directive runs must be idempotent too: a formatter that
            // re-escaped on the second pass would corrupt an already-clean file.
            "a {{ x.crv }} b {{ y.crv }} c {{ z.crv }} d\n",
            "a {{ two.crv #intro }} b {{ shift.crv @shift:2 }} c\n",
            "a {{ x.crv }} b {{ }} c {{ y.crv }} d\n",
        ] {
            let once = carve::to_carve(source);
            assert_eq!(carve::to_carve(&once), once, "not idempotent: {source:?}");
        }
    }
}

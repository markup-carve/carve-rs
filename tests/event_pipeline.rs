use carve::{parse_experimental_event_blocks, parse_with_options, render_html, to_json, Options};
#[cfg(feature = "experimental-event-parser-options")]
use carve::{
    parse_experimental_event_blocks_with_options, render_html_with_options, AsciiHeadingIds,
    IncludeOptions, Profile,
};
use std::fs;
use std::path::Path;

fn parse_standard(source: &str) -> carve::Document {
    parse_with_options(source, &Options::default())
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn option_aware_event_entry_preserves_supported_and_fallback_output() {
    for (source, options) in [
        (
            "# Étage\n\nSee </#Étage>.\n",
            Options::default().with_lowercase_heading_ids(true),
        ),
        (
            "# Étage\n\nSee </#Etage>.\n",
            Options::default().with_ascii_heading_ids(AsciiHeadingIds::Strict),
        ),
        (
            "See @alice and #topic.\n",
            Options::default()
                .with_mention_url("https://people.example/{name}")
                .with_tag_url("https://tags.example/{name}"),
        ),
        (
            "# Notes\n\nSee[^n].\n\n[^n]: Note.\n",
            Options::default().with_lowercase_heading_ids(true),
        ),
    ] {
        let (candidate, admitted) = parse_experimental_event_blocks_with_options(source, &options);
        assert!(admitted, "{source:?}");
        assert_eq!(
            candidate,
            parse_with_options(source, &options),
            "{source:?}"
        );
        assert_eq!(
            render_html_with_options(&candidate, &options).unwrap(),
            render_html_with_options(&parse_with_options(source, &options), &options).unwrap(),
            "{source:?}"
        );
    }
    let source = "# Heading\n\nText.\n";
    let options = Options::default().with_source_lines(true);
    let (candidate, admitted) = parse_experimental_event_blocks_with_options(source, &options);
    assert!(admitted);
    assert_eq!(candidate, parse_with_options(source, &options));
    for options in [
        Options::default().with_profile(Profile::default()),
        Options::default().with_lint_includes(IncludeOptions::new()),
    ] {
        let (candidate, admitted) = parse_experimental_event_blocks_with_options(source, &options);
        assert!(admitted);
        assert_eq!(candidate, parse_with_options(source, &options));
    }
    let options = Options::default().with_positions(true);
    let (candidate, admitted) = parse_experimental_event_blocks_with_options(source, &options);
    assert!(admitted);
    assert_eq!(candidate, parse_with_options(source, &options));
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn supported_options_match_across_direct_block_families() {
    for source in [
        "# Heading\n\nParagraph.\n",
        "- first\n- second\n",
        "> quoted\n",
        ":: term\n:  meaning\n",
        "Text[^n]\n\n[^n]: Note.\n",
        "```rust\ncode\n```\n",
        "[ref]: /target\n\nSee [ref][].\n",
    ] {
        for options in [
            Options::default(),
            Options::default().with_lowercase_heading_ids(true),
            Options::default().with_ascii_heading_ids(AsciiHeadingIds::Strict),
            Options::default()
                .with_mention_url("https://people.example/{name}")
                .with_tag_url("https://tags.example/{name}"),
        ] {
            let (candidate, admitted) =
                parse_experimental_event_blocks_with_options(source, &options);
            assert!(admitted, "{source:?}");
            let baseline = parse_with_options(source, &options);
            assert_eq!(candidate, baseline, "{source:?}");
            assert_eq!(
                render_html_with_options(&candidate, &options).unwrap(),
                render_html_with_options(&baseline, &options).unwrap(),
                "{source:?}"
            );
        }
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn positioned_event_entry_matches_editor_spans() {
    let sources = [
        ("\u{feff}# Étage\r\n\r\nCafé /é/.\r\n", true),
        ("# Heading\n\nOne\nTwo\n", true),
        ("- one\n- two\n", true),
        ("> quote\n> more\n", true),
        ("```rust\ncode\n```\n", true),
        (":: term\n:  meaning\n", true),
        ("::: note\nbody\n:::\n", true),
        ("![alt](/image)\n", true),
        ("[ref]: /target\n\nSee [ref][].\n", true),
        ("\u{feff}[ref]: /target\r\n\r\nSee [ref][].\r\n", true),
        ("A literal ]: token.\n", true),
        ("*[dl]: definition list\n\nA dl.\n", false),
        ("[^note]: footnote\n\nSee [^note].\n", true),
        ("[^note]: intro\n  > quote\n\nSee [^note].\n", true),
        ("\u{feff}[^note]: footnote\r\n\r\nSee [^note].\r\n", true),
        ("| A | B |\n| - | - |\n| 1 | 2 |\n", true),
        ("{table header-rows=0}\n```tsv\nA\tB\n```\n", true),
        ("{table}\n```csv\na,b\n```\n^ Cap\n", true),
        ("> {table}\n> ```csv\n> a,b\n> ```\n> ^ Cap\n", true),
        ("::: note\n{table}\n```csv\na,b\n```\n^ Cap\n:::\n", true),
        ("{.a}\n---\n", true),
        ("$$`x`\n", true),
        ("{#eq-emc}\n$$`E = mc^2`\n^ Equation #: mass-energy\n", true),
        ("```tsv\nA\tB\n```\n", true),
    ];
    for (source, expected_admission) in sources {
        for source_lines in [false, true] {
            let options = Options::default()
                .with_positions(true)
                .with_source_lines(source_lines);
            let (candidate, admitted) =
                parse_experimental_event_blocks_with_options(source, &options);
            assert_eq!(
                admitted, expected_admission,
                "{source:?}; source_lines={source_lines}"
            );
            let baseline = parse_with_options(source, &options);
            assert_eq!(
                candidate, baseline,
                "{source:?}; source_lines={source_lines}"
            );
            assert_eq!(
                render_html_with_options(&candidate, &options).unwrap(),
                render_html_with_options(&baseline, &options).unwrap(),
                "{source:?}; source_lines={source_lines}"
            );
        }
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn source_line_footnotes_match_standard_document() {
    for source in [
        "[^note]: footnote\n\nSee [^note].\n",
        "[^note]: intro\n  > quote\n\nSee [^note].\n",
    ] {
        let options = Options::default().with_source_lines(true);
        let (candidate, admitted) = parse_experimental_event_blocks_with_options(source, &options);
        assert!(admitted, "{source:?}");
        assert_eq!(
            candidate,
            parse_with_options(source, &options),
            "{source:?}"
        );
    }
}

#[test]
fn unsupported_root_literals_preserve_block_ownership() {
    for source in ["--\n", ": stray definition\n", "::: \tnote\n", "  {#id}\n"] {
        let (candidate, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        assert_eq!(candidate, parse_standard(source), "{source:?}");
    }
    let nested = "  - :: term\n    : definition\n\n~~~\ncode\n~~~\n";
    let (candidate, admitted) = parse_experimental_event_blocks(nested);
    assert!(!admitted);
    assert_eq!(candidate, parse_standard(nested));
}

#[test]
fn post_blank_list_line_returns_to_root_at_authored_column() {
    for source in [
        "- one\n\n > q\n",
        "- one\n\n  > q\n",
        "- one\n\n   # h\n",
        "- one\n > q\n",
    ] {
        let (candidate, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let baseline = parse_standard(source);
        assert_eq!(candidate, baseline, "{source:?}");
        assert_eq!(to_json(&candidate), to_json(&baseline), "{source:?}");
        assert_eq!(
            render_html(&candidate),
            render_html(&baseline),
            "{source:?}"
        );
    }
    let mut admitted_generated = 0;
    for marker in ["-", "*", "1."] {
        for blank in ["", "\n", "\n\n"] {
            for indent in 0..=4 {
                for tail in ["> q", "# h", "plain", "```"] {
                    let source = format!("{marker} one\n{blank}{}{tail}\n", " ".repeat(indent));
                    let (candidate, admitted) = parse_experimental_event_blocks(&source);
                    if admitted {
                        admitted_generated += 1;
                        let baseline = parse_standard(&source);
                        assert_eq!(candidate, baseline, "{source:?}");
                        assert_eq!(to_json(&candidate), to_json(&baseline), "{source:?}");
                        assert_eq!(
                            render_html(&candidate),
                            render_html(&baseline),
                            "{source:?}"
                        );
                    }
                }
            }
        }
    }
    assert!(admitted_generated >= 150);
    let tab_after_space = "- one\n\n \t> q\n";
    let (candidate, _) = parse_experimental_event_blocks(tab_after_space);
    let baseline = parse_standard(tab_after_space);
    assert_eq!(candidate, baseline);
    assert_eq!(to_json(&candidate), to_json(&baseline));
    assert_eq!(render_html(&candidate), render_html(&baseline));
    let repeated = (0..40)
        .map(|index| format!("- item {index}\n\n > quote {index}\n\n"))
        .collect::<String>();
    let (candidate, admitted) = parse_experimental_event_blocks(&repeated);
    assert!(admitted);
    let baseline = parse_standard(&repeated);
    assert_eq!(candidate, baseline);
    assert_eq!(to_json(&candidate), to_json(&baseline));
    assert_eq!(render_html(&candidate), render_html(&baseline));
}

#[test]
fn indented_root_text_preserves_the_full_ast() {
    for source in [
        " y\n",
        "  word\n",
        "- one\n\n y\n",
        ":: t\n:  definition\n\n  after\n",
    ] {
        let (candidate, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        assert_eq!(candidate, parse_standard(source), "{source:?}");
    }
    for source in [" x. x\n", " I. x\n", " iv) x\n"] {
        let (candidate, _) = parse_experimental_event_blocks(source);
        assert_eq!(candidate, parse_standard(source), "{source:?}");
    }
}

#[test]
fn nested_list_abbreviation_line_stays_in_list_item() {
    for source in [
        "> - a\n>\n>   a\n> *[X]: y\n>\n>   more\n",
        "::: note\n- a\n\n  a\n*[X]: y\n\n  more\n:::\n",
    ] {
        let (candidate, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        assert_eq!(candidate, parse_standard(source), "{source:?}");
    }
}

#[test]
fn repeated_marker_line_nested_lists_match_full_ast() {
    let source = (0..300)
        .map(|index| {
            format!("- - item {index}\n\n    continuation {index}\n  - sibling {index}\n\n")
        })
        .collect::<String>();
    let (candidate, admitted) = parse_experimental_event_blocks(&source);
    assert!(admitted);
    assert_eq!(candidate, parse_standard(&source));
}

#[test]
fn indented_root_owner_matrix_matches_full_document() {
    let leads = [
        "",
        "- item\n",
        "- item\n\n",
        "1. item\n\n",
        "> quote\n",
        "> quote\n\n",
        ":: term\n:  definition\n",
        "%% c\n",
        "%% c\n\n",
        "```\nx\n```\n",
        "::: note\npara\n:::\n",
    ];
    let tails = [
        "a",
        "y",
        "a b",
        "a :symbol:",
        "a `code`",
        "a\ncontinued",
        "a\n  b",
        "a. x",
        "iv) x",
        "P. S.",
    ];
    let mut admitted = 0;
    for lead in leads {
        for indent in 1..=5 {
            for tail in tails {
                for trailing in ["\n", "\n\nnext\n"] {
                    let source = format!("{lead}{}{tail}{trailing}", " ".repeat(indent));
                    let (candidate, used) = parse_experimental_event_blocks(&source);
                    if used {
                        admitted += 1;
                        assert_eq!(candidate, parse_standard(&source), "{source:?}");
                    }
                }
            }
        }
    }
    assert!(admitted >= 700);
}

#[test]
fn exact_corpus_ast_and_html() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut admitted = 0usize;
    let mut total = 0usize;
    for entry in fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("crv") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        let baseline = parse_standard(&source);
        let (candidate, used) = parse_experimental_event_blocks(&source);
        admitted += usize::from(used);
        total += 1;
        if used {
            assert_eq!(candidate, baseline, "{}", path.display());
        }
        assert_eq!(
            to_json(&candidate),
            to_json(&baseline),
            "{}",
            path.display()
        );
        assert_eq!(
            render_html(&candidate),
            render_html(&baseline),
            "{}",
            path.display()
        );
    }
    eprintln!("event parser admitted {admitted}/{total} corpus files");
    assert!(total > 3000, "the spec corpus must be available");
    assert!(admitted > 100, "the event route must exercise corpus files");
}

#[test]
fn shared_crossref_index_preserves_heading_ids_and_hrefs() {
    for source in [
        "# Heading\n\nSee </#heading>.\n",
        "# Heading\n\n# Heading\n\nSee </#heading> and </#heading-2>.\n",
        "# Heading {#chosen}\n\nSee </#chosen>.\n",
        "# Heading\n\n[x]{#heading}\n\n# Heading\n\nSee </#heading-2>.\n",
        "[x]{#heading}\n\n# Heading\n\nSee </#heading-2>.\n",
        "[x]{#intro}\n\n# intro\n\nSee </#intro>.\n",
        "{#intro}\n> q\n\n# intro\n\nSee </#intro>.\n",
        "{#intro}\n```\ncode\n```\n\n# intro\n\nSee </#intro>.\n",
        "{#intro}\n- item\n\n# intro\n\nSee </#intro>.\n",
        "{#intro}\n***\n\n# intro\n\nSee </#intro>.\n",
        "{#intro}\n![Alt](image.png)\n\n# intro\n\nSee </#intro>.\n",
        "> # Quoted\n\nSee </#quoted>.\n",
        "{#fig-sun}\n![A sunset](sun.jpg)\n^ Figure #: A *bold* -- sunset\n\nSee </#fig-sun>.\n",
    ] {
        let (candidate, used) = parse_experimental_event_blocks(source);
        assert!(used, "{source:?}");
        let baseline = parse_standard(source);
        assert_eq!(candidate, baseline, "{source:?}");
        assert_eq!(
            render_html(&candidate),
            render_html(&baseline),
            "{source:?}"
        );
    }
    for title in [
        "A *bold* label",
        "A `code` label",
        "A [link](https://example.org) label",
        "A \\*literal\\* label",
        "A -- dash label",
    ] {
        let source = format!("# {title} {{#target}}\n\nSee </#target>.\n");
        let (candidate, used) = parse_experimental_event_blocks(&source);
        assert!(used, "{source:?}");
        let baseline = parse_standard(&source);
        assert_eq!(candidate, baseline, "{source:?}");
        assert_eq!(
            render_html(&candidate),
            render_html(&baseline),
            "{source:?}"
        );
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn option_aware_event_corpus_matches_full_document() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let options = [
        Options::default().with_lowercase_heading_ids(true),
        Options::default().with_ascii_heading_ids(AsciiHeadingIds::Strict),
        Options::default()
            .with_mention_url("https://people.example/{name}")
            .with_tag_url("https://tags.example/{name}"),
        Options::default().with_source_lines(true),
        Options::default()
            .with_source_lines(true)
            .with_lowercase_heading_ids(true),
        Options::default()
            .with_source_lines(true)
            .with_ascii_heading_ids(AsciiHeadingIds::Strict),
        Options::default()
            .with_source_lines(true)
            .with_mention_url("https://people.example/{name}")
            .with_tag_url("https://tags.example/{name}"),
        Options::default().with_profile(Profile::default()),
        Options::default().with_lint_includes(IncludeOptions::new()),
        Options::default().with_positions(true),
        Options::default()
            .with_positions(true)
            .with_source_lines(true),
    ];
    let mut admitted = [0usize; 11];
    let mut total = 0usize;
    for entry in fs::read_dir(corpus).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("crv") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        total += 1;
        for (index, options) in options.iter().enumerate() {
            let (candidate, used) = parse_experimental_event_blocks_with_options(&source, options);
            admitted[index] += usize::from(used);
            assert_eq!(
                candidate,
                parse_with_options(&source, options),
                "{}",
                path.display()
            );
        }
    }
    assert!(total > 3000);
    assert!(
        admitted[..3].iter().all(|count| *count > 2000),
        "{admitted:?}"
    );
    assert!(
        admitted[3..7].iter().all(|count| *count > 2500),
        "{admitted:?}"
    );
    assert_eq!(admitted[7], admitted[0], "{admitted:?}");
    assert_eq!(admitted[8], admitted[0], "{admitted:?}");
    assert!(admitted[9] > 2200, "{admitted:?}");
    assert!(admitted[10] > 2200, "{admitted:?}");
}

#[test]
fn direct_route_admits_prose_and_uses_existing_list_reader() {
    let prose = "# Heading\n\nA paragraph with *strong* text.\n";
    let (doc, admitted) = parse_experimental_event_blocks(prose);
    assert!(admitted);
    assert_eq!(to_json(&doc), to_json(&parse_standard(prose)));

    let list = "- item\n";
    let (doc, admitted) = parse_experimental_event_blocks(list);
    assert!(admitted);
    assert_eq!(to_json(&doc), to_json(&parse_standard(list)));
}

#[test]
fn hash_prose_does_not_turn_a_list_heading_into_a_paragraph() {
    let prose = "#notaheading\n";
    let (actual, admitted) = parse_experimental_event_blocks(prose);
    assert!(admitted);
    assert_eq!(to_json(&actual), to_json(&parse_standard(prose)));

    for heading_in_item in [
        "- # In an item\n\nSee [In an item][].\n",
        "- ## In an item\n\nSee [In an item][].\n",
        "> - a\n>\n>   ### b ###\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(heading_in_item);
        assert!(admitted, "{heading_in_item:?}");
        let expected = parse_standard(heading_in_item);
        assert_eq!(to_json(&actual), to_json(&expected));
        assert_eq!(render_html(&actual), render_html(&expected));
    }
}

#[test]
fn headings_inside_list_frames_match_public_output() {
    let source = (0..120)
        .map(|index| format!("- item {index}\n  # Heading {index}\n"))
        .collect::<String>();
    let (actual, admitted) = parse_experimental_event_blocks(&source);
    assert!(admitted);
    let expected = parse_standard(&source);
    assert_eq!(to_json(&actual), to_json(&expected));
    assert_eq!(render_html(&actual), render_html(&expected));
}

#[test]
fn root_block_images_and_captions_use_event_route() {
    for source in [
        "![Apollo 11](apollo.jpg)\n",
        "![Apollo 11](apollo.jpg)",
        "![Apollo 11](apollo.jpg)\n^ Figure 1: First moon landing\n",
        "![Apollo 11](apollo.jpg)\n^ Figure 1: First moon landing",
        "![Apollo 11](apollo.jpg){.hero}\n^ Figure 1: First moon landing\n",
        "{.gallery}\n![Apollo 11](apollo.jpg)\n^ Figure 1: First moon landing\n",
        "![Apollo 11](apollo.jpg)\n\n^ Figure 1: First moon landing\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }

    for image in [
        "![alt](/image.png)",
        "![alt](/image.png){.hero}",
        "![alt][ref]",
        "![alt](/image.png) trailing",
    ] {
        for gap in ["\n", "\n\n", "\n\n\n"] {
            for caption in [
                "^ cap\n",
                "^ Figure #: cap\n",
                "^ cap\ncontinued\n",
                "^ cap\n- list\n",
                "^ cap\n> quote\n",
            ] {
                let source = format!("{image}{gap}{caption}");
                let (actual, _) = parse_experimental_event_blocks(&source);
                let expected = parse_standard(&source);
                assert_eq!(actual, expected, "{source:?}");
                assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
                assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
            }
        }
    }
    let (actual, admitted) = parse_experimental_event_blocks("![alt][ref]\n^ cap\n");
    assert!(admitted);
    assert_eq!(actual, parse_standard("![alt][ref]\n^ cap\n"));
    let source = "![alt](/image.png)\n\n\n^ cap\n";
    let (actual, admitted) = parse_experimental_event_blocks(source);
    assert!(admitted);
    assert_eq!(actual, parse_standard(source));
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn indented_image_with_flush_caption_keeps_figure() {
    for source in [
        "  ![a](b)\n^ cap\n",
        "  ![a](b)\n\n^ cap\n",
        "  ![a][r]\n^ cap\n\n[r]: /x\n",
        "> x\n>\n>   ![a](b)\n> ^ cap\n",
    ] {
        for options in [
            Options::default(),
            Options::default().with_source_lines(true),
            Options::default().with_positions(true),
            Options::default()
                .with_positions(true)
                .with_source_lines(true),
        ] {
            let (candidate, admitted) =
                parse_experimental_event_blocks_with_options(source, &options);
            assert!(admitted, "{source:?}");
            assert_eq!(
                candidate,
                parse_with_options(source, &options),
                "{source:?}"
            );
        }
    }
}

#[test]
fn root_comments_use_event_route() {
    for source in [
        "%% line comment\n",
        "%% line comment",
        "%%%\nfenced comment\n%%%\n",
        "%%%\nfenced comment\n%%%",
        "%%% opening text\nbody\n%%%\n",
        "%% comment\n\nAfterward.\n",
        "{.lead}\n%% comment\n\nAfterward.\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    for source in [
        "%%%\nno closer\n",
        "Paragraph.\n%% comment\n",
        "%% comment\n- item\n",
        "  %% indented\n",
        "%% comment\n![alt](image.png)\n",
    ] {
        let (actual, _) = parse_experimental_event_blocks(source);
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    let (actual, admitted) = parse_experimental_event_blocks("Paragraph.\n%% comment\n");
    assert!(admitted);
    assert_eq!(actual, parse_standard("Paragraph.\n%% comment\n"));
}

#[test]
fn long_fenced_list_matches_public_ast_and_html() {
    let mut source = String::new();
    for index in 0..300 {
        source.push_str(&format!(
            "- item {index}\n  ```rs\n  let x = {index};\n  ```\n"
        ));
    }
    let (actual, admitted) = parse_experimental_event_blocks(&source);
    assert!(admitted);
    let expected = parse_standard(&source);
    assert_eq!(to_json(&actual), to_json(&expected));
    assert_eq!(render_html(&actual), render_html(&expected));
}

#[test]
fn unified_list_owner_keeps_loose_and_nested_boundaries() {
    for source in [
        "- first\n\n  second paragraph\n",
        "- first\n\n  second paragraph\n\n- next\n",
        "1. first\n\n   second paragraph\n\n2. next\n",
        "- first\n\n  - nested\n- next\n",
        "- first\n\n  second paragraph\n\nRoot paragraph\n",
        "- a\n  - nested\n\n- b\n",
        "1. a\n   - nested\n\n2. b\n",
        "- a\n  - nested\n\nRoot paragraph\n",
        "- a\n  - nested\n\n\nRoot paragraph\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn unified_list_owner_builds_fully_marked_quotes() {
    for source in [
        "- one\n  > q\n",
        "- one\n\n  > q\n",
        "- one\n\n  >\t\n",
        "- one\n  > q\n  > r\n- next\n",
        "- one\n  > ```\n  > code\n  > ```\n",
        "- one\n  > ```\n  > code  \n  > ```\n",
        "- one\n  > - nested\n",
        "1. one\n   > q\n2. next\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn list_quotes_keep_the_standard_nesting_cutoff() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let mut failures = Vec::new();
            for depth in [180, 185, 190, 194, 195, 196, 197, 198, 199, 200, 205, 210] {
                for last in ["q", "- a"] {
                    let source = format!("- one\n  {}{last}\n", "> ".repeat(depth));
                    let (actual, used) = parse_experimental_event_blocks(&source);
                    let expected = parse_standard(&source);
                    if to_json(&actual) != to_json(&expected)
                        || render_html(&actual) != render_html(&expected)
                    {
                        failures.push((depth, last, used));
                    }
                }
            }
            for depth in [180, 198, 205] {
                for source in [
                    format!("- outer\n  - inner\n    {}q\n", "> ".repeat(depth)),
                    format!("> - one\n>   {}q\n", "> ".repeat(depth)),
                ] {
                    let (actual, used) = parse_experimental_event_blocks(&source);
                    let expected = parse_standard(&source);
                    if to_json(&actual) != to_json(&expected)
                        || render_html(&actual) != render_html(&expected)
                    {
                        failures.push((depth, "nested", used));
                    }
                }
            }
            assert!(failures.is_empty(), "{failures:?}");
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn marked_quotes_with_fences_keep_exact_output() {
    for source in [
        "> ```\n> code\n> ```\n",
        "> ```=html\n> <b>x</b>\n> ```\n",
        "> before\n>\n> ```rust\n> let x = 1;\n> ```\n",
        "> ```\n> unclosed\n",
        "> > ````\n> > code\n> > ````\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    for (source, admitted) in [("> ```=html\n>", false), ("> ```=html\n> ", false)] {
        let (actual, used) = parse_experimental_event_blocks(source);
        assert_eq!(used, admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    for opener in ["```", "```=html", "~~~rust"] {
        for payload in [">", "> ", ">   ", "> text"] {
            for closer in ["", "> ```\n"] {
                for ending in ["", "\n"] {
                    let source = format!("> {opener}\n{payload}\n{closer}{ending}");
                    let (actual, _) = parse_experimental_event_blocks(&source);
                    let expected = parse_standard(&source);
                    assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
                    assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
                }
            }
        }
    }
}

#[test]
fn generated_unified_list_owners_keep_exact_output() {
    let fragments = ["plain", "*strong*", "`code`", "a & b"];
    let mut state = 0x4a77_6d21_u32;
    let mut admitted = 0;
    for case in 0..512 {
        let ordered = case % 2 == 0;
        let marker = if ordered { "1. " } else { "- " };
        let prefix = if ordered { "   " } else { "  " };
        let mut source = format!("{marker}first\n\n");
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        source.push_str(prefix);
        source.push_str(fragments[(state as usize) % fragments.len()]);
        source.push('\n');
        if case % 3 == 0 {
            source.push('\n');
            source.push_str(prefix);
            source.push_str("another paragraph\n");
        }
        if case % 5 == 0 {
            source.push('\n');
            source.push_str(prefix);
            source.push_str("- nested\n");
        }
        if case % 2 == 1 {
            source.push('\n');
            source.push_str("- next\n");
        }
        if case % 7 == 0 {
            source.push_str("\nAfter.\n");
        }
        let (actual, used) = parse_experimental_event_blocks(&source);
        admitted += usize::from(used);
        let expected = parse_standard(&source);
        assert_eq!(
            to_json(&actual),
            to_json(&expected),
            "case {case}: {source:?}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&expected),
            "case {case}: {source:?}"
        );
    }
    assert!(
        admitted > 200,
        "only {admitted} cases used the direct route"
    );
}

#[test]
fn unified_list_boundaries_keep_exact_output() {
    for source in [
        "- a\n\n   three-space paragraph\n",
        "1. a\n\n  two-space paragraph\n",
        "- a\n\n\ttab paragraph\n",
        "- a\n\n  : definition\n",
        "- a\n\n* b\n",
        "- a\n\n- - -\n",
        "1. one\n2. two\n3. three\n4. four\n5. five\n6. six\n7. seven\n8. eight\n9. nine\n10. ten\n",
        "- a\nlazy continuation\n",
        "- a\n\n  second paragraph\nlazy continuation\n",
    ] {
        let (actual, _) = parse_experimental_event_blocks(source);
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn deep_list_at_parser_cap_keeps_exact_output() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for depth in [197, 198] {
                let mut source = String::new();
                for level in 0..depth {
                    source.push_str(&"  ".repeat(level));
                    source.push_str("- item\n");
                }
                let (actual, _) = parse_experimental_event_blocks(&source);
                let expected = parse_standard(&source);
                assert_eq!(to_json(&actual), to_json(&expected), "depth {depth}");
                assert_eq!(
                    render_html(&actual),
                    render_html(&expected),
                    "depth {depth}"
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn literal_double_star_labels_keep_exact_ast_and_html() {
    for source in [
        "**Core syntax**\n",
        "**Tier 2**\n\nAfter.\n",
        "**bold**\n",
        "**a*b**\n",
        "**with (c)**\n",
        "**\u{e9}clair**\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn isolated_root_link_definitions_keep_public_nodes_and_references() {
    for source in [
        "A [link][r].\n\n[r]: /u\n",
        "[r]: /u\n\n# Heading\n\nA [link][r].\n",
        "[r]: /u\n[s]: /v\n\nA [link][r] and [other][s].\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    for source in [
        "[r]: /first\n[r]: /second\n\nA [link][r].\n",
        "```\n[r]: /u\n",
        "A [r]: text in a paragraph.\n\n[s]: /u\n",
    ] {
        let (actual, _) = parse_experimental_event_blocks(source);
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn normalized_source_keeps_direct_ast_html_and_original_byte_length() {
    for ending in ["\n", "\r\n", "\r"] {
        for bom in ["", "\u{feff}"] {
            let source = format!("{bom}# Heading{ending}{ending}Plain *strong* text.{ending}");
            let expected = parse_standard(&source);
            let (actual, admitted) = parse_experimental_event_blocks(&source);
            assert!(admitted, "line ending {ending:?}, BOM {}", !bom.is_empty());
            assert_eq!(actual.source_len, source.len());
            assert_eq!(to_json(&actual), to_json(&expected));
            assert_eq!(render_html(&actual), render_html(&expected));
        }
        let source = format!("A\0B{ending}");
        let expected = parse_standard(&source);
        let (actual, admitted) = parse_experimental_event_blocks(&source);
        assert!(admitted, "NUL with line ending {ending:?}");
        assert_eq!(actual.source_len, source.len());
        assert_eq!(to_json(&actual), to_json(&expected));
        assert_eq!(render_html(&actual), render_html(&expected));
    }
}

#[test]
fn normalization_probe_keeps_sentinel_and_benign_unicode_boundaries() {
    for benign in [
        "# Heading\n\nCafé.\n",
        "# Heading\n\nWide，punctuation.\n",
        "# Heading\n\nX\u{e007}Y\n",
        "# Heading\n\nX\u{e000}Y\n",
        "# Heading\n\nX\u{fefe}Y\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(benign);
        assert!(admitted, "{benign:?}");
        assert_eq!(to_json(&actual), to_json(&parse_standard(benign)));
    }
    for source in [
        "# Heading\n\nX\u{e005}Y\n",
        "# Heading\n\nX\u{e006}Y\n",
        "Text[^n]\n\n[^n]: Note\u{e005}.\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(!admitted, "{source:?}");
        assert_eq!(
            to_json(&actual),
            to_json(&parse_standard(source)),
            "{source:?}"
        );
    }
}

#[test]
fn mixed_route_handles_root_quote_without_changing_output() {
    let source = "# Heading\n\n> quoted *text*\n> second line\n\nAfter.\n";
    let (actual, admitted) = parse_experimental_event_blocks(source);
    assert!(admitted);
    let expected = parse_standard(source);
    assert_eq!(to_json(&actual), to_json(&expected));
    assert_eq!(render_html(&actual), render_html(&expected));
}

#[test]
fn fully_marked_quote_lists_keep_exact_output() {
    for source in [
        "> - item\n",
        "> - alpha\n> - beta\n",
        "> * alpha\n> * beta\n",
        "> 1. alpha\n> 2. beta\n",
        "> - alpha\n>   beta\n> - gamma\n",
        "> 1. alpha\n>    beta\n",
        "> - alpha\n>   : beta\n",
        "> - *strong* and `code`\n",
        "> - a[^1]\n\n[^1]: note\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn marked_quote_bodies_share_the_block_route() {
    for source in [
        "> > inner\n",
        "> > inner\n> > more\n",
        "> lead\n>\n> > inner\n",
        "> # Heading\n>\n> - item\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }

    let deep = format!("{}text\n", "> ".repeat(48));
    let expected = parse_standard(&deep);
    let (actual, _) = parse_experimental_event_blocks(&deep);
    assert_eq!(to_json(&actual), to_json(&expected));
    assert_eq!(render_html(&actual), render_html(&expected));
}

#[test]
fn generated_marked_quote_bodies_keep_exact_output() {
    let lines = [
        "plain",
        "",
        "# Heading",
        "- item",
        "  wrapped",
        "1. ordered",
        "> nested",
        "---",
        ":: term",
        ":  definition",
        "{.class}",
        "```",
        "code",
        "::: note",
        "[ref]: /url",
        "a [ref][x]",
        "![alt](x)",
        "%% comment",
    ];
    let mut state = 0x6c13_4f29_u32;
    let mut admitted = 0;
    for case in 0..4_096 {
        let mut source = String::new();
        for _ in 0..1 + case % 7 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            source.push('>');
            let line = lines[(state as usize) % lines.len()];
            if !line.is_empty() {
                source.push(' ');
                source.push_str(line);
            }
            source.push('\n');
        }
        let (actual, used) = parse_experimental_event_blocks(&source);
        admitted += usize::from(used);
        let expected = parse_standard(&source);
        assert_eq!(
            to_json(&actual),
            to_json(&expected),
            "case {case}: {source:?}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&expected),
            "case {case}: {source:?}"
        );
    }
    assert!(admitted > 30, "quote bodies must exercise the event route");
}

#[test]
fn mixed_route_admits_root_table_without_changing_output() {
    let source = "# Heading\n\n|= Fruit |= Price |\n| Apple | $1 |\n^ Fruit prices\n\nAfter.\n";
    let (actual, admitted) = parse_experimental_event_blocks(source);
    assert!(admitted);
    let expected = parse_standard(source);
    assert_eq!(to_json(&actual), to_json(&expected));
    assert_eq!(render_html(&actual), render_html(&expected));
}

#[test]
fn plain_definition_entries_build_directly() {
    for source in [
        ":: alpha\n:  beta\n",
        ":: alpha\n:  beta\n:: gamma\n:  delta\n",
        ":: alpha *bold*\n:  beta `code`\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    for source in [
        ":: alpha\n:  a. beta\n",
        ":: alpha\n:  i. beta\n",
        ":: alpha\n:  a) beta\n",
        ":: alpha\n:  iv. beta\n",
    ] {
        let (actual, _) = parse_experimental_event_blocks(source);
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn generated_plain_definition_entries_keep_exact_output() {
    let terms = [
        "term",
        "term *strong*",
        "term `code`",
        "term [link](url)",
        "term (c)",
        "term {#id}",
    ];
    let defs = [
        "definition",
        "definition /italic/",
        "definition `code`",
        "definition [link](url)",
        "definition (tm)",
        "definition **strong**",
    ];
    let mut state = 0x93a6_718b_u32;
    for case in 0..4_096 {
        let mut source = String::new();
        for _ in 0..1 + case % 5 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            source.push_str(":: ");
            source.push_str(terms[(state as usize) % terms.len()]);
            source.push_str("\n:  ");
            source.push_str(defs[((state >> 8) as usize) % defs.len()]);
            source.push('\n');
        }
        if case % 2 == 0 {
            source.pop();
        }
        let (actual, admitted) = parse_experimental_event_blocks(&source);
        assert!(admitted, "case {case}: {source:?}");
        let expected = parse_standard(&source);
        assert_eq!(
            to_json(&actual),
            to_json(&expected),
            "case {case}: {source:?}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&expected),
            "case {case}: {source:?}"
        );
    }
}

#[test]
fn mixed_route_admits_root_sigil_blocks_without_changing_output() {
    for source in [
        "::: |\none\ntwo\n:::\n",
        "::: \\\none\ntwo\n:::\n",
        "::: >\nquoted\n:::\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn mixed_route_admits_nested_list_markers_when_reader_owns_them() {
    for source in [
        "- outer\n  - inner\n",
        "# Heading\n\n- outer\n  - inner\n\nAfter.\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn deep_plain_lists_keep_sibling_ownership_and_exact_output() {
    for source in [
        "- parent\n  - child\n    - one\n    - two\n  - next child\n    - three\n- next parent\n  - last child\n    - four\n",
        "1. parent\n   1. child\n      1. one\n      2. two\n   2. next child\n      1. three\n",
        "- parent\n  - child\n    - one\n\nafter\n",
        "- parent\n  - child\n    - one\nlazy\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    for root_ordered in [false, true] {
        for child_ordered in [false, true] {
            for grand_ordered in [false, true] {
                let marker = |ordered: bool, number: usize| {
                    if ordered {
                        format!("{number}. ")
                    } else {
                        "- ".to_owned()
                    }
                };
                let root = marker(root_ordered, 1);
                let child = marker(child_ordered, 1);
                let grand = marker(grand_ordered, 1);
                let source = format!(
                    "{root}parent\n{root_indent}{child}child\n{grand_indent}{grand}one *emphasis*\n{grand_indent}{grand_next}two\n{root_indent}{child_next}next child\n{grand_indent}{grand}three\n{root_next}next parent\n",
                    root_indent = " ".repeat(root.len()),
                    grand_indent = " ".repeat(root.len() + child.len()),
                    grand_next = marker(grand_ordered, 2),
                    child_next = marker(child_ordered, 2),
                    root_next = marker(root_ordered, 2),
                );
                let (actual, admitted) = parse_experimental_event_blocks(&source);
                assert!(admitted, "{source:?}");
                let expected = parse_standard(&source);
                assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
                assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
            }
        }
    }
}

#[test]
fn event_route_resolves_collapsed_heading_references() {
    for source in [
        "# a :smile: b\n\n[a b][]\n",
        "[Later][]\n\n# Later\n",
        "# Same\n\n# Same\n\n[Same][]\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn event_route_lifts_root_link_definitions() {
    for source in [
        "[site][ref]\n\n[ref]: https://example.com\n",
        "[ref]: https://example.com \"title\"\n\n[site][ref]\n",
        "[ref]: https://first.example\n[ref]: https://second.example\n\n[ref][]\n",
        "# Heading\n\n[Heading][]\n\n[ref]: https://example.com\n",
        "[ref]: https://example.com\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn event_route_lifts_simple_footnotes() {
    for source in [
        "Text[^n]\n\n[^n]: Note.\n",
        "# Notes\n\nSee[^one] and[^two].\n\n[^one]: First.\n[^two]: Second.\n",
        "Text[^n]\n\n[^n]: *emphasized* note.\n",
        "Text[^n]\n\n[^n]: First line.\n    Second line.\n",
        "Text[^n]\n\n[^n]: # Heading in a note\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn normalized_footnotes_preserve_authored_source_length() {
    for source in [
        "Text[^n]\r\n\r\n[^n]: Note.\r\n",
        "\u{feff}Text[^n]\n\n[^n]: Note.\n",
        "Text[^n]\0\n\n[^n]: Note.\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(actual.source_len, source.len(), "{source:?}");
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
        assert!(carve::authored_provenance(&actual, source, None).is_ok());
    }
}

#[test]
fn event_route_lifts_root_abbreviations() {
    for source in [
        "The HTML spec is long.\n*[HTML]: HyperText Markup Language\n",
        "*[API]: Application Programming Interface\n\nAn API call.\n",
        "# HTML\n\n*[HTML]: HyperText Markup Language\n\nHTML again.\n",
        "Text[^n] uses HTML.\n\n[^n]: Note.\n\n*[HTML]: HyperText Markup Language\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn event_route_attaches_root_attributes() {
    for source in [
        "{#install .featured}\n## Setup\n",
        "{.fancy}\n```php\ncode\n```\n",
        "{#x}\n\nParagraph.\n",
        "{#x}\n{.y}\nText.\n",
        "{#x}\n*[HTML]: HyperText\n\nHTML text.\n",
        "{table header-rows=0}\n```tsv\nA\tB\n```\n",
        "{.fancy}\n``` rust \"Example\"\ncode\n```\n",
        "{title=\"A\"}\n``` rust \"B\"\ncode\n```\n^ Cap\n",
        "{#x .a}\n*[HTML]: HyperText\n{.b}\nHTML text.\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn root_attributes_before_a_footnote_keep_exact_output() {
    let source = "{.a}\n[^1]: note\n\nText[^1].\n";
    let (actual, _) = parse_experimental_event_blocks(source);
    let expected = parse_standard(source);
    assert_eq!(to_json(&actual), to_json(&expected));
    assert_eq!(render_html(&actual), render_html(&expected));
}

#[test]
fn nested_footnote_definitions_keep_exact_output() {
    for (source, expected_admission) in [
        (
            "Text[^outer]\n\n[^outer]: Inside[^inner].\n[^inner]: Nested.\n",
            true,
        ),
        ("- a[^n]\n\n  [^n]: note\n- b\n", false),
        ("1. a[^n]\n\n   [^n]: note\n2. b\n", false),
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert_eq!(admitted, expected_admission, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn nested_footnote_definitions_keep_source_lines_and_positions() {
    for source in [
        "Text[^outer]\n\n[^outer]: Inside[^inner].\n[^inner]: Nested.\n",
        "[^outer]: intro\n\n     [^inner]: note\n\n     see[^inner]\n\nsee[^outer]\n",
    ] {
        for options in [
            Options::default(),
            Options::default().with_source_lines(true),
            Options::default().with_positions(true),
            Options::default()
                .with_positions(true)
                .with_source_lines(true),
        ] {
            let (candidate, admitted) =
                parse_experimental_event_blocks_with_options(source, &options);
            assert!(admitted, "{source:?}");
            assert_eq!(
                candidate,
                parse_with_options(source, &options),
                "{source:?}"
            );
        }
    }
}

#[test]
fn quote_marker_and_raw_fence_boundaries_keep_exact_output() {
    for source in [
        "> - a\n>+\n> tail\n",
        "> ```=html\n> ",
        "> ```=html\n> \n",
        "> a\n\n^ caption\n",
        "> quote\n*[HTML]: HyperText\n\nHTML\n",
        "> quote HTML\n*[HTML]: HyperText\n",
        "> a\n> b\n*[HTML]: HyperText\n\nHTML\n",
        "> a\nlazy\n*[HTML]: HyperText\n\nHTML\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert_eq!(admitted, source != "> ```=html\n> ", "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn root_colon_containers_keep_exact_output_on_event_route() {
    for source in [
        "::: note\nA paragraph.\n:::\n",
        "::: note\n> quoted\n:::\n",
        "::: figure\nA paragraph.\n:::\n",
        "::: note\n::: note\nPlain paragraph.\n:::\n:::\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn marked_quote_edges_keep_exact_output() {
    for source in ["> # H\n> text\n", ">\n>\n", "> a\n\n> b", "> plain"] {
        let (actual, _) = parse_experimental_event_blocks(source);
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn bounded_lazy_quote_paragraphs_keep_exact_output() {
    for marked_lines in [1, 2] {
        for lazy_lines in [1, 2] {
            for suffix in ["", "\nAfter.\n"] {
                let mut source = String::new();
                for _ in 0..marked_lines {
                    source.push_str("> marked text\n");
                }
                for _ in 0..lazy_lines {
                    source.push_str("plain continuation\n");
                }
                source.push_str(suffix);
                let (actual, admitted) = parse_experimental_event_blocks(&source);
                assert!(admitted, "{source:?}");
                let expected = parse_standard(&source);
                assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
                assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
            }
        }
    }
}

#[test]
fn punctuation_led_paragraphs_and_task_lists_keep_exact_nodes() {
    let prose = "/italic/\n\n_underline_\n\n[link](target)\n\n![image](target)\n";
    let (actual, admitted) = parse_experimental_event_blocks(prose);
    assert!(admitted);
    assert_eq!(to_json(&actual), to_json(&parse_standard(prose)));
    assert_eq!(render_html(&actual), render_html(&parse_standard(prose)));

    let task_list = "- [ ] > q\n- [x] # h\n- [ ] ---\n- # H\n";
    let (actual, _) = parse_experimental_event_blocks(task_list);
    assert_eq!(to_json(&actual), to_json(&parse_standard(task_list)));
    assert_eq!(
        render_html(&actual),
        render_html(&parse_standard(task_list))
    );

    for first in ["/italic/", "_underline_", "~strike~", "=mark="] {
        let source = format!("# Heading\n\n{first}\nnext line\n");
        let (actual, admitted) = parse_experimental_event_blocks(&source);
        assert!(admitted, "{first}");
        assert_eq!(
            to_json(&actual),
            to_json(&parse_standard(&source)),
            "{first}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&parse_standard(&source)),
            "{first}"
        );
    }

    for next in ["[link](target)", "![image](target)", "\\*escaped"] {
        let source = format!("first line\n{next}\n");
        let (actual, admitted) = parse_experimental_event_blocks(&source);
        assert!(admitted, "{next}");
        assert_eq!(
            to_json(&actual),
            to_json(&parse_standard(&source)),
            "{next}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&parse_standard(&source)),
            "{next}"
        );
    }

    for item in [
        "/italic/",
        "_underline_",
        "~strike~",
        "=mark=",
        "\\*escaped",
        "![image](target)",
    ] {
        let source = format!("- {item}\n- next\n");
        let (actual, admitted) = parse_experimental_event_blocks(&source);
        assert!(admitted, "{item}");
        assert_eq!(
            to_json(&actual),
            to_json(&parse_standard(&source)),
            "{item}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&parse_standard(&source)),
            "{item}"
        );
    }
}

#[test]
fn invalid_brace_lines_remain_paragraphs_in_the_event_route() {
    for source in ["{_x_}\n", "{_x_} y\n", "{+⃣x+} {+y+}\n"] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }

    let valid_attrs = "{.class}\n# Heading\n";
    let (actual, admitted) = parse_experimental_event_blocks(valid_attrs);
    assert!(admitted);
    assert_eq!(to_json(&actual), to_json(&parse_standard(valid_attrs)));
}

#[test]
fn two_level_tight_lists_keep_exact_ast_and_html() {
    for parent in ["-", "*", "1."] {
        for child in ["-", "*", "1."] {
            for parent_count in [1, 2] {
                for child_count in [1, 2] {
                    for continuation_lines in [0, 1, 2] {
                        let indent = if parent == "1." { "   " } else { "  " };
                        let mut source = String::new();
                        for _ in 0..parent_count {
                            source.push_str(&format!("{parent} outer\n"));
                            for _ in 0..continuation_lines {
                                source.push_str(&format!("{indent}continued\n"));
                            }
                            for index in 0..child_count {
                                let marker = if child == "1." {
                                    format!("{}.", index + 1)
                                } else {
                                    child.to_string()
                                };
                                source.push_str(&format!("{indent}{marker} inner\n"));
                            }
                        }
                        let (actual, admitted) = parse_experimental_event_blocks(&source);
                        assert!(admitted, "{source:?}");
                        let expected = parse_standard(&source);
                        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
                        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn nested_list_keeps_indented_blocks_after_a_blank() {
    for source in [
        "- a\n  - b\n\n   > q\n",
        "- outer\n  - head\n\n    ```\n    a\n  ```\n\n  tail\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn generated_prose_keeps_exact_ast_and_html() {
    let fragments = [
        "plain words",
        "*strong*",
        "/italic/",
        "[link](https://example.org)",
        "`code`",
        "a & b",
        "(c) (r) (tm)",
        "a\\*b",
        "{=mark=}",
        "--",
        "...",
        "\"quote\"",
    ];
    let mut state = 0x9e37_79b9_u32;
    let mut admitted = 0usize;
    for case in 0..1024 {
        let mut source = String::new();
        if case % 3 == 0 {
            source.push_str("# Generated heading\n\n");
        }
        for line in 0..1 + case % 4 {
            source.push('A');
            for _ in 0..1 + (case + line) % 5 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                source.push(' ');
                source.push_str(fragments[(state as usize) % fragments.len()]);
            }
            source.push('\n');
        }
        if case % 2 == 0 {
            source.push('\n');
        }
        let expected = parse_standard(&source);
        let (actual, used) = parse_experimental_event_blocks(&source);
        admitted += usize::from(used);
        assert_eq!(
            to_json(&actual),
            to_json(&expected),
            "case {case}: {source:?}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&expected),
            "case {case}: {source:?}"
        );
    }
    assert!(
        admitted > 0,
        "generated cases must exercise the event route"
    );
}

#[test]
fn long_literal_lexer_keeps_ast_and_html_parity() {
    let safe = [
        "plain words and punctuation, one: two; three? yes!",
        "Unicode text: café, Zürich, 日本語 and “curly quotes”",
        "carve#529 and foo:bar are ordinary prose here",
        "safe hyphenated words and a & b",
    ];
    let unsafe_cases = [
        "an ellipsis ... must reach the existing inline parser",
        "a dash -- and (c) must reach the existing inline parser",
        "*strong* and `code` must reach the existing inline parser",
    ];
    for case in 0..256 {
        let mut source = String::from("# Heading\n\nA long paragraph starts here. ");
        for index in 0..8 {
            source.push_str(safe[(case + index * 3) % safe.len()]);
            source.push(' ');
        }
        if case % 2 == 1 {
            source.push_str(unsafe_cases[(case / 2) % unsafe_cases.len()]);
        }
        source.push('\n');
        let expected = parse_standard(&source);
        let (actual, admitted) = parse_experimental_event_blocks(&source);
        assert!(admitted, "case {case}");
        assert_eq!(to_json(&actual), to_json(&expected), "case {case}");
        assert_eq!(render_html(&actual), render_html(&expected), "case {case}");
    }
}

#[test]
fn authored_documents_match_when_available() {
    let Some(root) = std::env::var_os("CARVE_DOCS_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    for relative in ["terms.md", "case-study/parsing-ast.md", "spec-history.md"] {
        let path = root.join(relative);
        let source = fs::read_to_string(&path).unwrap();
        let baseline = parse_standard(&source);
        let (candidate, used) = parse_experimental_event_blocks(&source);
        assert!(used, "{} did not enter the event route", path.display());
        assert_eq!(
            to_json(&candidate),
            to_json(&baseline),
            "{}",
            path.display()
        );
        assert_eq!(
            render_html(&candidate),
            render_html(&baseline),
            "{}",
            path.display()
        );
    }
}

#[test]
fn generated_flat_lists_keep_exact_ast_and_html() {
    let fragments = [
        "plain",
        "*strong*",
        "`code`",
        "[link](https://example.org)",
        "a & b",
    ];
    let mut state = 0x51ed_4d13_u32;
    for case in 0..512 {
        let ordered = case % 2 == 0;
        let mut source = String::from("# List\n\n");
        for item in 0..1 + case % 7 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let text = fragments[(state as usize) % fragments.len()];
            if ordered {
                source.push_str(&format!("{}. {text}\n", item + 1));
            } else {
                source.push_str(&format!("- {text}\n"));
            }
            if case % 3 == 0 && item % 2 == 0 {
                source.push_str(if ordered {
                    "   wrapped words\n"
                } else {
                    "  wrapped words\n"
                });
            }
        }
        source.push_str("\nAfter the list.\n");
        let expected = parse_standard(&source);
        let (actual, used) = parse_experimental_event_blocks(&source);
        assert!(used, "case {case} declined");
        assert_eq!(
            to_json(&actual),
            to_json(&expected),
            "case {case}: {source:?}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&expected),
            "case {case}: {source:?}"
        );
    }
}

#[test]
fn generated_block_combinations_keep_exact_ast_and_html() {
    let lines = [
        "A plain sentence.",
        "# Heading",
        "- item",
        "  wrapped",
        "1. ordered",
        ":: term",
        ":  definition",
        "```",
        "code",
        "~~~",
        "---",
        "> quote",
        "+",
        "[ref]: /url",
        "A [ref][x]",
        "::: note",
        ":::",
        "{.class}",
        "A ...",
        "**bold**",
        "A {^note^}",
        "![alt](x)",
        "A paragraph",
        "",
        "",
        "A /italic/",
        "%% comment",
        "  - nested",
        "A ![alt](x)",
        "A (c) and (tm)",
    ];
    let mut state = 0x7ab3_902d_u32;
    let mut admitted = 0usize;
    for case in 0..16_384 {
        let mut source = String::new();
        for _ in 0..2 + case % 9 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            source.push_str(lines[(state as usize) % lines.len()]);
            source.push('\n');
        }
        let expected = parse_standard(&source);
        let (actual, used) = parse_experimental_event_blocks(&source);
        admitted += usize::from(used);
        assert_eq!(
            to_json(&actual),
            to_json(&expected),
            "case {case}: {source:?}"
        );
        assert_eq!(
            render_html(&actual),
            render_html(&expected),
            "case {case}: {source:?}"
        );
    }
    assert!(
        admitted > 100,
        "generated blocks must exercise the event route"
    );
}

#[test]
fn blank_before_complex_list_item_keeps_one_list() {
    for source in [
        "- a\n\n- - -\n",
        "- a\n\n-  b\n",
        "1. a\n\n2.  b\n",
        "- a\n\n- # h\n",
    ] {
        let expected = parse_standard(source);
        let (actual, _) = parse_experimental_event_blocks(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn marked_quote_fence_grid_matches_public_ast_and_html() {
    let mut admitted = 0usize;
    for (open, close) in [
        ("```", "```"),
        ("~~~", "~~~"),
        ("```js", "```"),
        ("```=html", "```"),
    ] {
        for payload in ["x", "", "one\n> two", "::: note"] {
            for tail in ["", "\nOutside.\n", "> More.\n"] {
                let source = format!("> {open}\n> {payload}\n> {close}\n{tail}");
                let expected = parse_standard(&source);
                let (actual, used) = parse_experimental_event_blocks(&source);
                admitted += usize::from(used);
                assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
                assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
            }
        }
    }
    assert!(admitted > 0, "the grid must exercise the event route");
}

#[test]
fn quoted_fences_without_a_final_newline_use_the_standard_route() {
    for source in [
        "> ```\n> code",
        "- > ```\n  > code",
        "1. > ```\n   > code",
        "- > ```=html\n  > ",
        "a[^1]\n\n> ```\n> code",
        "[^1]: ```\n    code",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(!admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    for source in ["> ```\n> code\n", "- > ```\n  > code\n"] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        let expected = parse_standard(source);
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
}

#[test]
fn quoted_fence_reader_boundaries_match_public_ast_and_html() {
    let mut admitted = 0usize;
    for body in [
        "> ```\n> x\n",
        "> ```\n> x\n> ```\n",
        "> ~~~js\n> x\n> ~~~\n",
        "> ::: |\n> ```\n> x\n",
        "> text\n> ```\n",
        "> - item\n> ```\n> x\n",
        "> ::: note\n> ```\n> x\n> :::\n",
        "> ```=html\n> x\n",
    ] {
        for tail in [
            "after\n",
            "> again\n",
            "\nAfter.\n",
            "- item\n",
            ":  desc\n",
            "+\n",
            "# Heading\n",
            "> ```\n",
            "",
        ] {
            for prefix in ["", "Before.\n\n", "# Heading\n\n", "- item\n\n"] {
                let source = format!("{prefix}{body}{tail}");
                let expected = parse_standard(&source);
                let (actual, used) = parse_experimental_event_blocks(&source);
                admitted += usize::from(used);
                assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
                assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
            }
        }
    }
    assert!(admitted > 0, "the grid must exercise the event route");
}

#[test]
fn root_continuation_owner_grid_matches_public_ast_and_html() {
    for source in [
        ":: term\n: A first paragraph,\n+\nthen a flush-left block joined with +.\n",
        "> quoted\n+\n- item\n",
    ] {
        let expected = parse_standard(source);
        let (actual, used) = parse_experimental_event_blocks(source);
        assert!(used, "root continuation owner must enter the event route");
        assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
        assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
    }
    let mut admitted = 0usize;
    for prefix in [
        "",
        "Text\n",
        "# Heading\n",
        "- item\n",
        "> quote\n",
        ":: term\n:  desc\n",
        "::: note\ninside\n:::\n",
        "---\ntitle: T\n---\n\n",
    ] {
        for marker in ["+\n", " +\n", "- +\n", "> +\n", ":  +\n"] {
            for follower in [
                "```\na\n```\n",
                "> quote\n",
                "- item\n",
                "Plain.\n",
                "::: note\nbody\n:::\n",
            ] {
                for tail in ["", "\nAfter.\n", "\n:: next\n:  x\n"] {
                    let source = format!("{prefix}{marker}{follower}{tail}");
                    let expected = parse_standard(&source);
                    let (actual, used) = parse_experimental_event_blocks(&source);
                    admitted += usize::from(used);
                    assert_eq!(to_json(&actual), to_json(&expected), "{source:?}");
                    assert_eq!(render_html(&actual), render_html(&expected), "{source:?}");
                }
            }
        }
    }
    assert!(admitted > 0, "the grid must exercise the event route");
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn delegated_paragraph_owner_keeps_exact_document() {
    let cases = [
        "Prose\n```js title=\"x\"\ncode\n```\n",
        "![a][ok]\n^ cap\n\n[ok]: /p.png\n",
        "x\n  %% indented comment\ny\n",
        "Prose\n- item\nnext\n",
        "A paragraph\n:: term\n:  meaning\n",
        "Text\n{.class}\n\nAfter.\n",
    ];
    for source in cases {
        for options in [
            Options::default(),
            Options::default().with_source_lines(true),
            Options::default().with_positions(true),
            Options::default()
                .with_positions(true)
                .with_source_lines(true),
        ] {
            let (candidate, admitted) =
                parse_experimental_event_blocks_with_options(source, &options);
            assert!(admitted, "{source:?}");
            assert_eq!(
                candidate,
                parse_with_options(source, &options),
                "{source:?}"
            );
        }
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn generated_delegated_paragraph_boundaries_keep_exact_document() {
    let fragments = [
        "plain",
        "  indented",
        "- item",
        ":: term\n:  meaning",
        "```x\ny\n```",
        "%% comment",
        "{.a}",
        "![a](b)",
        "^ caption",
        "> quoted",
        "::: note\nhi\n:::",
    ];
    let mut state = 0x6283_f911_u32;
    let mut admitted = 0;
    for case in 0..4096 {
        let mut source = String::new();
        for _ in 0..4 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if !source.is_empty() && (state & 3) == 0 {
                source.push('\n');
            }
            source.push_str(fragments[(state as usize) % fragments.len()]);
            source.push('\n');
        }
        for options in [
            Options::default(),
            Options::default().with_source_lines(true),
        ] {
            let (candidate, used) = parse_experimental_event_blocks_with_options(&source, &options);
            admitted += usize::from(used);
            assert_eq!(
                candidate,
                parse_with_options(&source, &options),
                "case {case}: {source:?}"
            );
        }
    }
    assert!(admitted > 5000, "only {admitted} cases admitted");
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn nested_footnote_markers_survive_link_definition_guard() {
    let nested = [
        "x[^c]\n\n:: t\n: [^c]: para\n    # H\n",
        "- > para\n  > [^f]: n\n  >   - sub\ntail[^f]\n",
        "x[^c]\n\n:: t\n: [^c]: para\n     > q\ntail\n",
    ];
    for source in nested {
        let (plain, admitted) = parse_experimental_event_blocks(source);
        assert!(admitted, "{source:?}");
        assert_eq!(plain, parse_standard(source), "{source:?}");
        for options in [
            Options::default(),
            Options::default().with_source_lines(true),
            Options::default().with_positions(true),
            Options::default()
                .with_positions(true)
                .with_source_lines(true),
        ] {
            let (candidate, admitted) =
                parse_experimental_event_blocks_with_options(source, &options);
            assert!(admitted, "{source:?}");
            assert_eq!(
                candidate,
                parse_with_options(source, &options),
                "{source:?}"
            );
        }
    }
    for source in [
        "See [x][id] and [^n].\n\n[id]: /x\n\n[^n]: note\n",
        "x[^n]\n\n[^n]: [x][id]\n  [id]: /x\n",
    ] {
        let (candidate, admitted) = parse_experimental_event_blocks(source);
        assert!(!admitted, "{source:?}");
        assert_eq!(candidate, parse_standard(source), "{source:?}");
        for options in [
            Options::default(),
            Options::default().with_source_lines(true),
            Options::default().with_positions(true),
            Options::default()
                .with_positions(true)
                .with_source_lines(true),
        ] {
            let (candidate, admitted) =
                parse_experimental_event_blocks_with_options(source, &options);
            assert!(!admitted, "{source:?}");
            assert_eq!(
                candidate,
                parse_with_options(source, &options),
                "{source:?}"
            );
        }
    }
}

#[cfg(feature = "experimental-event-parser-options")]
#[test]
fn nested_note_and_link_definition_prefixes_keep_exact_document() {
    let options = [
        Options::default(),
        Options::default().with_source_lines(true),
        Options::default().with_positions(true),
        Options::default()
            .with_positions(true)
            .with_source_lines(true),
    ];
    for prefix in ["", "  ", "> ", "- ", ": ", "- > "] {
        for tail in [
            "[^n]: note",
            "[id]: /target",
            "[^n]: [x][id]",
            "[id]: /target\n[^n]: note",
            "[^n]: note\n  [id]: /target",
            "[^n]: [x][id]\n  [id]: /target",
        ] {
            let source = format!("See [^n] and [x][id].\n\n{prefix}{tail}\n");
            for option in &options {
                let (candidate, _) = parse_experimental_event_blocks_with_options(&source, option);
                assert_eq!(candidate, parse_with_options(&source, option), "{source:?}");
            }
        }
    }
}

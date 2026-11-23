use carve::Options;
use carve::lint::{LintOptions, lint};

fn ranges(source: &str, code: &str) -> Vec<(u32, u32)> {
    let options = LintOptions {
        upgrade: true,
        ..LintOptions::default()
    };
    lint(source, &options)
        .into_iter()
        .filter(|finding| finding.code == code)
        .filter_map(|finding| finding.pos)
        .map(|pos| (pos.start_offset, pos.end_offset))
        .collect()
}

#[test]
fn quoted_list_columns_ignore_indent_before_the_quote_mark() {
    for source in [
        "> - a\n>\n  >   # H\n",
        "> i) :::\n  >   :: term\n",
        "> - :::\n  >  :: term\n",
        "> - a\n>\n\t>   # H\n",
    ] {
        assert!(
            ranges(source, "list-item-block-overindented").is_empty(),
            "{source:?}"
        );
    }
}

#[test]
fn attribute_lines_are_not_heading_continuations() {
    for source in ["   {.x}\n   ### H\n", "{#id}\n= H\n"] {
        assert!(
            ranges(source, "deprecated-indented-line-under-heading").is_empty(),
            "{source:?}"
        );
    }
    assert_eq!(
        ranges("   {.x}\n   ### H\n", "block-opener-overindented"),
        vec![(11, 14)]
    );
}

#[test]
fn comments_after_lazy_quote_markers_are_quote_candidates() {
    assert_eq!(
        ranges(">-\n> %%x\n", "block-opener-without-blank-line"),
        vec![(3, 4)]
    );
    assert_eq!(
        ranges("  c\n  > %%%\n", "block-opener-without-blank-line"),
        vec![(6, 7)]
    );
    assert_eq!(
        ranges("a\n> \n", "block-opener-without-blank-line"),
        vec![(2, 3)]
    );
}

#[test]
fn unclosed_container_range_covers_its_block() {
    assert_eq!(
        ranges("abc\n::: note\nbody\n", "unclosed-container-fence"),
        vec![(4, 17)]
    );
}

#[test]
fn render_losses_keep_each_construct_and_its_position() {
    let source = "a\n\n{{ m.crv }}\n\nb\n\n> q\n";
    let rendered = carve::render_with_diagnostics(
        source,
        &Options {
            profile: carve::options::Profile::Minimal,
            ..Options::default()
        },
    );
    let losses: Vec<_> = rendered
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "profile-construct-degraded")
        .collect();
    assert_eq!(losses.len(), 2);
    assert_eq!(losses[0].pos.unwrap().start_offset, 3);
    assert_eq!(losses[1].pos.unwrap().start_offset, 19);
    assert!(
        losses[0]
            .message
            .contains("\"minimal\" does not allow: include")
    );
    assert!(
        losses[1]
            .message
            .contains("\"minimal\" does not allow: block_quote")
    );

    let includes = carve::render_with_diagnostics(
        "a\n\n{{ m.crv }}\n\nb\n\n{{ n.crv }}\n",
        &Options::default(),
    );
    let positions: Vec<_> = includes
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "include-unresolved")
        .map(|finding| finding.pos.unwrap().start_offset)
        .collect();
    assert_eq!(positions, [3, 19]);
}

#[test]
fn generated_variants_keep_the_correct_owner_and_span() {
    let minimal = Options {
        profile: carve::options::Profile::Minimal,
        ..Options::default()
    };
    for index in 0..1000 {
        let indent = " ".repeat(index % 4);
        let heading = "#".repeat(1 + index % 6);
        let lazy = format!("{indent}text{index}\n{indent}> %%%\n");
        let mark = lazy.find('>').unwrap();
        assert_eq!(
            ranges(&lazy, "block-opener-without-blank-line"),
            vec![(
                u32::try_from(mark).unwrap(),
                u32::try_from(mark + 1).unwrap()
            )],
            "lazy variant {index}"
        );
        let empty_quote = format!("text{index}\n{indent}> \n");
        let mark = empty_quote.find('>').unwrap();
        assert_eq!(
            ranges(&empty_quote, "block-opener-without-blank-line"),
            vec![(
                u32::try_from(mark).unwrap(),
                u32::try_from(mark + 1).unwrap()
            )],
            "empty quote variant {index}"
        );

        let quoted = format!("> - a{index}\n>\n{indent}>   {heading} H\n");
        assert!(
            ranges(&quoted, "list-item-block-overindented").is_empty(),
            "quoted item variant {index}"
        );

        let attributed = format!("{indent}{{#id{index}}}\n{indent}{heading} H\n");
        assert!(
            ranges(&attributed, "deprecated-indented-line-under-heading").is_empty(),
            "heading variant {index}"
        );

        let div = format!("lead{index}\n::: note\nbody{index}\n");
        let start = u32::try_from(div.find(":::").unwrap()).unwrap();
        let end = u32::try_from(div.trim_end_matches('\n').len()).unwrap();
        assert_eq!(
            ranges(&div, "unclosed-container-fence"),
            vec![(start, end)],
            "div variant {index}"
        );

        let degraded = format!("lead{index}\n\n{{{{ m{index}.crv }}}}\n\n> q{index}\n");
        let rendered = carve::render_with_diagnostics(&degraded, &minimal);
        let profiles: Vec<_> = rendered
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "profile-construct-degraded")
            .collect();
        assert_eq!(profiles.len(), 2, "profile variant {index}");
        assert_eq!(
            profiles[0].pos.unwrap().start_offset,
            u32::try_from(degraded.find("{{").unwrap()).unwrap(),
            "profile include variant {index}"
        );
        assert_eq!(
            profiles[1].pos.unwrap().start_offset,
            u32::try_from(degraded.find("> q").unwrap()).unwrap(),
            "profile quote variant {index}"
        );
    }
}

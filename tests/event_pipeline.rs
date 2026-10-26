use carve::{parse_experimental_event_blocks, parse_with_options, render_html, to_json, Options};
use std::fs;
use std::path::Path;

fn parse_standard(source: &str) -> carve::Document {
    parse_with_options(source, &Options::default())
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
fn mixed_route_handles_root_quote_without_changing_output() {
    let source = "# Heading\n\n> quoted *text*\n> second line\n\nAfter.\n";
    let (actual, admitted) = parse_experimental_event_blocks(source);
    assert!(admitted);
    let expected = parse_standard(source);
    assert_eq!(to_json(&actual), to_json(&expected));
    assert_eq!(render_html(&actual), render_html(&expected));
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
fn quote_marker_and_raw_fence_boundaries_use_standard_parser() {
    for source in [
        "> - a\n>+\n> tail\n",
        "> ```=html\n> ",
        "> ```=html\n> \n",
        "> a\n\n^ caption\n",
        "> > inner\n",
        "> - item\n",
    ] {
        let (actual, admitted) = parse_experimental_event_blocks(source);
        assert!(!admitted, "{source:?}");
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
    assert!(!admitted);
    assert_eq!(to_json(&actual), to_json(&parse_standard(valid_attrs)));
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

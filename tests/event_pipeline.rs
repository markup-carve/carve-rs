use carve::{parse, parse_experimental_event_blocks, render_html, to_json};
use std::fs;
use std::path::Path;

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
        let baseline = parse(&source);
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
fn direct_route_is_admitted_for_prose_and_rejects_containers() {
    let prose = "# Heading\n\nA paragraph with *strong* text.\n";
    let (doc, admitted) = parse_experimental_event_blocks(prose);
    assert!(admitted);
    assert_eq!(to_json(&doc), to_json(&parse(prose)));

    let list = "- item\n";
    let (doc, admitted) = parse_experimental_event_blocks(list);
    assert!(!admitted);
    assert_eq!(to_json(&doc), to_json(&parse(list)));
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
        let expected = parse(&source);
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

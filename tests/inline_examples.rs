//! The inline pass against the oracle trees of the specification examples:
//! every top-level paragraph and heading is parsed from its source lines and
//! compared with the oracle's children, positions included.
mod common;
#[path = "support/inline.rs"]
mod support;

#[test]
#[ignore = "candidate 3 example oracle; candidate 12 tree fixtures are the core gate"]
fn inline_examples_match_oracle_paragraphs() {
    let mut total = 0;
    let mut single = 0;
    let mut single_failed = 0;
    let mut failed = Vec::new();
    for ex in common::examples() {
        // A profile filters nodes in the resolve stage, not here.
        if ex.opts.get("profile").is_some() {
            continue;
        }
        let expected_tree = ex.spec_tree.as_ref().unwrap_or(&ex.oracle_tree);
        let is_single = expected_tree
            .get("children")
            .and_then(|c| c.as_array())
            .is_some_and(|c| c.len() == 1);
        for (want, got) in support::compare(&ex.input, expected_tree, common::sorted) {
            total += 1;
            single += usize::from(is_single);
            if want != got {
                single_failed += usize::from(is_single);
                failed.push(format!("--- {}\n  want {want}\n  got  {got}\n", ex.id));
            }
        }
    }
    let mut report = format!(
        "paragraphs and headings: {}/{total} pass; single-paragraph examples: {}/{single} pass\n",
        total - failed.len(),
        single - single_failed
    );
    report.push_str(&failed.concat());
    if let Ok(path) = std::env::var("INLINE_REPORT") {
        let _ = std::fs::write(path, &report);
    }
    println!("{}", report.lines().next().unwrap_or(""));
    assert!(failed.is_empty(), "{report}");
}

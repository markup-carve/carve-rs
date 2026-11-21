//! A brace block that attaches to nothing is text: the data rules read no attribute records from it,
//! so `semantic-attribute-outside-span` does not fire on it (C2-B-192, C2-I-036).

use carve::lint::{LintOptions, lint};

fn codes(source: &str) -> Vec<&'static str> {
    lint(source, &LintOptions::default())
        .into_iter()
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn unattached_brace_blocks_carry_no_attribute_records() {
    // This integration test has one test process, so the internal switch is isolated.
    unsafe { std::env::set_var("CARVE_LINT_RULES", "data") };
    let semantic = "semantic-attribute-outside-span";
    for source in [
        "foo {abbr=\"x\"}\n",
        "foo{abbr=\"x\"}\n",
        "foo {.c abbr=\"x\"} tail\n",
        "> foo{abbr=\"x\" cite=\"u\"}\n",
        "{abbr=\"x\"}\n",
        "- {.c abbr=\"x\"}\n",
    ] {
        assert!(!codes(source).contains(&semantic), "{source:?}");
    }
    assert!(codes("{_e_}{abbr=\"x\"}\n").contains(&semantic));
    assert!(codes("{abbr=\"x\"}\nfoo\n").contains(&semantic));
}

//! Hand-written rules give the findings and wording of their catalog rows (carve#6186).

fn found(input: &str, code: &str) -> Vec<(u32, String)> {
    carve::lint::lint(input, &carve::lint::LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .map(|finding| (finding.pos.expect("range").start_offset, finding.message))
        .collect()
}

#[test]
fn continuation_plus_behind_any_item_marker() {
    for (input, at, here) in [
        ("1. +\n", 3, true),
        ("a) +\n", 3, true),
        ("- [ ] +\n", 6, true),
        ("1. > +\n", 5, false),
        ("* > > +\n", 6, false),
        ("    * > > +\n> >\n", 10, false),
    ] {
        let found = found(input, "deprecated-continuation-marker");
        assert_eq!(found.len(), 1, "{input:?}");
        assert_eq!(found[0].0, at, "{input:?}");
        let phrase = if here {
            "is text here"
        } else {
            "inside this list item"
        };
        assert!(found[0].1.contains(phrase), "{input:?}: {}", found[0].1);
    }
}

#[test]
fn overindented_opener_names_kind_and_owner_in_words() {
    for (input, kind, owner) in [
        (">  ::: x\n> :::\n", "div", "block quote"),
        ("::: x\n  :: t\n  : d\n:::\n", "definition term", "div"),
        (
            "::: note\n   ```\n   c\n   ```\n:::\n",
            "code block",
            "admonition",
        ),
        (" | a |\n", "table", "document"),
    ] {
        let found = found(input, "block-opener-overindented");
        assert_eq!(found.len(), 1, "{input:?}: {found:?}");
        let message = &found[0].1;
        assert!(
            message.starts_with(&format!("This {kind} is indented")),
            "{message}"
        );
        assert!(
            message.contains(&format!("content column of {owner},")),
            "{message}"
        );
    }
}

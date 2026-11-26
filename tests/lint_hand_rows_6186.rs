//! Hand-written rules give the findings and wording of their catalog rows (carve#6186).

fn found(input: &str, code: &str) -> Vec<(u32, String)> {
    // C2-W-156: `deprecated-continuation-marker` runs under `upgrade`.
    let options = carve::lint::LintOptions {
        upgrade: true,
        ..carve::lint::LintOptions::default()
    };
    carve::lint::lint(input, &options)
        .into_iter()
        .filter(|finding| finding.code == code)
        .map(|finding| (finding.pos.expect("range").start_offset, finding.message))
        .collect()
}

#[test]
fn continuation_plus_behind_any_item_marker() {
    for (input, at) in [
        ("1. +\n", 3),
        ("a) +\n", 3),
        ("- [ ] +\n", 6),
        ("- > - +\n", 6),
        ("> - +\n", 4),
    ] {
        let found = found(input, "deprecated-continuation-marker");
        assert_eq!(found.len(), 1, "{input:?}");
        assert_eq!(found[0].0, at, "{input:?}");
        assert!(
            found[0].1.contains("is text here"),
            "{input:?}: {}",
            found[0].1
        );
    }
    // The item's marker-line text is `> +`, not `+` (carve-rs#4530).
    for input in ["1. > +\n", "* > > +\n", "- [ ] > +\n"] {
        assert!(
            found(input, "deprecated-continuation-marker").is_empty(),
            "{input:?}"
        );
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

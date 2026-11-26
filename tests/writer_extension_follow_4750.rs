use carve::api::{Carve, Options, WriteOptions};

#[test]
fn repaired_extension_content_leaves_harmless_followers_bare() {
    let carve = Carve::new();
    for input in ["x :a[\\]]( y\n", ":a[\\[](/]\\\n", ":a[\\]]/\n"] {
        let tree = carve.parse(input, &Options::default()).unwrap();
        let written = carve
            .write_with_diagnostics(&tree, &WriteOptions::default())
            .unwrap();
        assert_eq!(written.output, input, "{input:?}");
    }
}

#[test]
fn repaired_extension_followers_across_contexts() {
    let carve = Carve::new();
    let mut checked = 0;
    for prefix in ["", "x ", "word ", "> ", "- "] {
        for name in ["a", "ab", "word", "ext"] {
            for content in [r"\]", r"\[", r"x\]", r"\]x", r"x\["] {
                for follower in ["(", "/", "( y", "/ y", "(/]"] {
                    for end in ["\n", "\\\n"] {
                        let input = format!("{prefix}:{name}[{content}]{follower}{end}");
                        let tree = carve.parse(&input, &Options::default()).unwrap();
                        let written = carve
                            .write_with_diagnostics(&tree, &WriteOptions::default())
                            .unwrap();
                        let again = carve.parse(&written.output, &Options::default()).unwrap();
                        assert_eq!(
                            serde_json::to_value(again.document()).unwrap(),
                            serde_json::to_value(tree.document()).unwrap(),
                            "{input:?} -> {:?}",
                            written.output
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}

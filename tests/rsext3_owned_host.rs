use carve::api::{Carve, Options, extensions};
use std::fmt::Write as _;

fn host() -> Carve {
    let mut host = Carve::new();
    host.register(extensions::Bundle::merge([
        extensions::presets(extensions::PresetBuilders::default()),
        extensions::math_block(extensions::MathBlockOptions::default()),
        extensions::code_group(extensions::CodeGroupOptions::default()),
        extensions::code_callouts(),
    ]))
    .unwrap();
    host
}

#[test]
fn fast_render_matches_the_public_tree_path_for_1024_claimed_variants() {
    let host = host();
    let opts = Options::default();
    let siblings = ["", "\nAfter.", "\n- an item", "\n## Next"];
    for number in 1..=4 {
        for container in 0..4 {
            for depth in 0..4 {
                for sibling in siblings {
                    for suffix in ["", " and *bold*", " [link](u)", " `code`"] {
                        let source = format!(
                            "```js\nlet x = {number}; // <{number}>\n```\n\n<{number}> The value{suffix}.{sibling}\n"
                        );
                        let source = match container {
                            1 => format!("::: note\n{source}:::\n"),
                            2 => format!("::: code-group\n{source}:::\n"),
                            3 => format!("A lead.\n\n{source}"),
                            _ => source,
                        };
                        let source = if depth == 0 {
                            source
                        } else {
                            let prefix = ">".repeat(depth);
                            source.lines().fold(String::new(), |mut out, line| {
                                let _ = writeln!(out, "{prefix} {line}");
                                out
                            })
                        };
                        let parsed = host.parse(source.as_str(), &opts).unwrap();
                        let public = host.render_tree(&parsed, &opts).unwrap();
                        let fast = host.render(source.as_str(), &opts).unwrap();
                        assert_eq!(
                            fast, public,
                            "variant {number}, {container}, {depth}, {sibling}, {suffix}"
                        );
                    }
                }
            }
        }
    }
}

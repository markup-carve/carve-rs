//! carve-rs#4366: the hosted render of a source runs in one arena. It must
//! give the HTML and the record of the public parse and tree render.
use carve::api::{Carve, Options, extensions};

fn hosts() -> Vec<Carve> {
    [
        "carve.details",
        "carve.wiki",
        "carve.codeGroup",
        "carve.listTable",
    ]
    .into_iter()
    .map(|name| {
        let mut host = Carve::new();
        host.enable_standard(name, true).unwrap();
        host
    })
    .chain(std::iter::once({
        let mut host = Carve::new();
        host.register(extensions::details()).unwrap();
        host.register(extensions::code_callouts()).unwrap();
        host
    }))
    .collect()
}

#[test]
fn arena_render_matches_the_tree_path() {
    let opts = Options::default();
    let bodies = [
        "::: details \"More\"\nHidden [^n].\n:::\n\n[^n]: note\n",
        "::: code-group\n```js [NPM]\nnpm i\n```\n\n```sh\nyarn add\n```\n:::\n",
        "See [[Home Page]] and [[Other|label]].\n",
        "::: list-table\n- - a\n  - b\n:::\n",
        "# A {#x}\n\n# B {#x}\n\nNo claim here.\n",
        "```js\nlet a = 1 <1>\n```\n<1> One\n",
    ];
    for host in hosts() {
        for body in bodies {
            for source in [
                body.to_string(),
                format!("> {}", body.replace('\n', "\n> ")),
                format!("- {}", body.replace('\n', "\n  ")),
            ] {
                let direct = host
                    .render_with_diagnostics(source.as_str(), &opts)
                    .unwrap();
                let parsed = host.parse_with_diagnostics(source.as_str(), &opts).unwrap();
                let tree = host
                    .render_tree_with_diagnostics(&parsed.tree, &opts)
                    .unwrap();
                assert_eq!(direct.html, tree.html, "{source}");
                let codes =
                    |d: &[carve::api::Entry]| d.iter().map(|e| e.code.clone()).collect::<Vec<_>>();
                let mut both = codes(&parsed.diagnostics);
                both.extend(codes(&tree.diagnostics));
                assert_eq!(codes(&direct.diagnostics), both, "{source}");
            }
        }
    }
}

/// A host claimer sees the resolved tree in a render as in `parse`: here the
/// generated id of a heading in the div body.
#[test]
fn a_host_claimer_reads_resolved_nodes_in_a_render() {
    struct Summary;
    impl carve::api::Renderer for Summary {
        fn render(
            &self,
            node: &serde_json::Value,
            cx: &carve::api::RenderContext<'_>,
        ) -> Result<String, carve::api::Error> {
            cx.render_inlines(&node["slots"]["summary"])
        }
    }
    let mut bundle = extensions::details();
    bundle.extensions[0].declaration.name = "test.idProbe".into();
    bundle.extensions[0].renderers = vec![(carve::api::Channel::Interactive, Box::new(Summary))];
    bundle.matchers[0].extension = "test.idProbe".into();
    bundle.matchers[0].claimer = Box::new(|claim: &extensions::Claim<'_>| {
        let id = claim
            .node
            .pointer("/children/0/generatedId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("none")
            .to_owned();
        let summary = serde_json::json!([{"type": "text", "value": format!("id {id}")}]);
        Some(serde_json::json!({
            "type": "block_extension", "name": "test.idProbe", "version": "1",
            "children": [], "slots": {"summary": summary, "body": []},
            "fallback": [{"type": "paragraph", "children": summary}],
        }))
    });
    let mut host = Carve::new();
    host.register(bundle).unwrap();
    let opts = Options::default();
    let source = "::: details\n## Hello\n:::\n";
    let direct = host.render(source, &opts).unwrap();
    let tree = host
        .render_tree(&host.parse(source, &opts).unwrap(), &opts)
        .unwrap();
    assert_eq!(direct, tree);
    assert!(direct.contains("id Hello"), "{direct}");
}

fn fenced_hosts() -> Vec<Carve> {
    let build = |kind: &'static str| -> extensions::DiagramBuilder {
        std::sync::Arc::new(move |src: &str| format!("<svg>{kind}:{}</svg>", src.len()))
    };
    let mut all = Carve::new();
    all.register(extensions::mermaid(extensions::PresetOptions::default()))
        .unwrap();
    all.register(extensions::math_block(
        extensions::MathBlockOptions::default(),
    ))
    .unwrap();
    all.register(extensions::code_group(
        extensions::CodeGroupOptions::default(),
    ))
    .unwrap();
    all.register(extensions::code_callouts()).unwrap();
    let mut statics = Carve::new();
    statics
        .register(extensions::mermaid(extensions::PresetOptions {
            build: Some(build("mermaid")),
            wrap_in_figure: true,
            ..Default::default()
        }))
        .unwrap();
    statics
        .register(extensions::chart(extensions::PresetOptions::default()))
        .unwrap();
    statics
        .register(extensions::math_block(extensions::MathBlockOptions {
            build: Some(std::sync::Arc::new(|tex: &str, _| {
                format!("<math>{tex}</math>")
            })),
            ..Default::default()
        }))
        .unwrap();
    statics
        .register(extensions::code_group(extensions::CodeGroupOptions {
            mode: extensions::CodeGroupMode::Aria,
            ..Default::default()
        }))
        .unwrap();
    vec![all, statics]
}

/// The typed claims of a source render give what the JSON claims of
/// `parse` give, and the typed renderers what the tree render gives.
#[test]
fn typed_claims_match_the_tree_path() {
    let bodies = [
        "{#m .wide role=note}\n```mermaid\ngraph TD; A-->B & <x>\n```\n",
        "{aria-label=Flow}\n```mermaid \"A *title*\"\ngraph\n```\n",
        "```chart\n{\"a\": \"</script>\"}\n```\n",
        "{.big role=x}\n```math\na < b\n```\n",
        "{#g .code-group .more selected=no}\n::: code-group\n%% note\n```js [NPM] {selected}\nnpm i\n```\n\n```mermaid\ngraph\n```\n:::\n",
        "::: code-group \"Install [r]\"\n```sh\nyarn <1>\n```\n:::\n<1> Run [r]\n\n[r]: /dest\n",
        "::: code-group\n```js\na\n```\n\nNot code.\n:::\n",
        "::: code-group\n:::\n",
        "{#c .x}\n```js\nlet a = 1 <1>\nlet b <2>\n```\n{.list}\n<1> *One*\n<2> Two [x](u)\n",
    ];
    let mut seen = std::collections::BTreeSet::new();
    for opts in [
        Options::default(),
        Options {
            source_lines: true,
            ..Options::default()
        },
        Options {
            mode: carve::api::Mode::Static,
            ..Options::default()
        },
    ] {
        for host in fenced_hosts() {
            for body in bodies {
                for source in [
                    body.to_string(),
                    format!("> {}", body.replace('\n', "\n> ")),
                    format!("- {}", body.replace('\n', "\n  ")),
                    format!("::: note\n{body}:::\n"),
                ] {
                    let direct = host
                        .render_with_diagnostics(source.as_str(), &opts)
                        .unwrap();
                    let parsed = host.parse_with_diagnostics(source.as_str(), &opts).unwrap();
                    let tree = host
                        .render_tree_with_diagnostics(&parsed.tree, &opts)
                        .unwrap();
                    assert_eq!(direct.html, tree.html, "{source}");
                    for mark in [
                        "class=\"mermaid",
                        "<svg>",
                        "math display",
                        "<math>",
                        "code-group",
                        "data-callout",
                        "role=\"tab\"",
                    ] {
                        if direct.html.contains(mark) {
                            seen.insert(mark);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(seen.len(), 7, "{seen:?}");
}

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

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

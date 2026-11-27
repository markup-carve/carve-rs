use carve::api::{self, Error, Options};
use carve::export::{self, ExportOptions, Target};

fn tree(source: &str) -> api::Tree {
    api::parse(source, &Options::default()).unwrap()
}

#[test]
fn exports_read_the_expanded_view_for_each_snippet_shape() {
    let cases = [
        ("{$v}: X\n\nA {{ $v }}.\n", "A X.\n", &[][..]),
        ("{$v}: X\n\n{{ $v }}\n", "X\n", &[][..]),
        ("{$v .c}: X\n\na {{ $v }}\n", "a X\n", &[][..]),
        (
            "a {{ $missing }}\n",
            "a {{ $missing }}\n",
            &["snippet-unresolved"][..],
        ),
        ("{$a}: {{ $b }}\n{$b}: X\n\n{{ $a }}\n", "X\n", &[][..]),
        (
            "{$a}: {{ $a }}\n\n{{ $a }}\n",
            "{{ $a }}\n",
            &["snippet-cycle"][..],
        ),
    ];
    for (source, output, expected_codes) in cases {
        let tree = tree(source);
        let authored = api::encode_tree(&tree);
        for target in [Target::Markdown, Target::Plain, Target::Ansi] {
            let written = export::export_to(target, &tree, &ExportOptions::default()).unwrap();
            assert_eq!(written.output, output, "{target:?}: {source:?}");
            let codes: Vec<&str> = written
                .diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect();
            assert_eq!(codes, expected_codes, "{target:?}: {source:?}");
        }
        assert_eq!(api::encode_tree(&tree), authored, "{source:?}");
    }
}

#[test]
fn unresolved_snippet_has_a_path_and_strict_export_refuses_its_loss() {
    let source = "a {{ $missing }}\n";
    let tree = tree(source);
    let result = export::to_plain_text(&tree, &ExportOptions::default()).unwrap();
    assert_eq!(
        result.diagnostics[0].path.as_deref(),
        Some("/children/0/children/1")
    );
    assert_eq!(result.diagnostics[0].channel, "render-loss");
    let strict = ExportOptions {
        strict_losses: true,
        ..ExportOptions::default()
    };
    assert!(matches!(
        export::to_plain_text(&tree, &strict),
        Err(Error::Export { diagnostics }) if diagnostics[0].code == "snippet-unresolved"
    ));
}

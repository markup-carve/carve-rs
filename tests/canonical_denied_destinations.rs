use carve::CheckedRenderOptions;
use serde_json::Value;

fn meaning(source: &str) -> Value {
    fn clear_positions(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.remove("pos");
                fields.remove("srcByteLength");
                for child in fields.values_mut() {
                    clear_positions(child);
                }
            }
            Value::Array(children) => {
                for child in children {
                    clear_positions(child);
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    clear_positions(&mut value);
    value
}

#[test]
fn canonical_source_preserves_denied_destinations() {
    for scheme in [
        "javascript",
        "JaVaScRiPt",
        "vbscript",
        "data",
        "file",
        "ms-msdt",
        "vscode",
    ] {
        for tail in ["alert(1)", "a(b(c))d", "a\\)b", "a\\(b", "a\\\\b"] {
            let source = format!("[x]({scheme}:{tail}) ![i]({scheme}:{tail})\n");
            let written = carve::to_carve_with_report(
                &source,
                CheckedRenderOptions {
                    strict: true,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(meaning(&written.value), meaning(&source), "{source}");
            assert!(written.losses.is_empty());
            assert_eq!(written.total_losses, 0);
            assert_eq!(carve::to_carve(&written.value), written.value);
            for render in [carve::to_html_with_report, carve::to_markdown_with_report] {
                let result = render(&written.value, CheckedRenderOptions::default()).unwrap();
                let codes: Vec<_> = result.losses.iter().map(|loss| loss.code).collect();
                assert_eq!(
                    codes,
                    ["destination-denied", "destination-denied"],
                    "{source}"
                );
            }
            let ansi = carve::to_ansi_with_report(&written.value, CheckedRenderOptions::default())
                .unwrap();
            let codes: Vec<_> = ansi.losses.iter().map(|loss| loss.code).collect();
            assert_eq!(codes, ["destination-denied"], "{source}");
        }
    }
}

#[test]
fn balanced_denied_parentheses_stay_readable() {
    let source = "[x](javascript:alert(1))\n";
    assert_eq!(carve::to_carve(source), source);
}

#[test]
fn reference_and_autolink_destinations_survive() {
    for source in [
        "[x][r]\n\n[r]: javascript:alert(1)\n",
        "<javascript:alert(1)>\n",
    ] {
        let written = carve::to_carve_with_report(
            source,
            CheckedRenderOptions {
                strict: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(meaning(&written.value), meaning(source));
        assert_eq!(written.total_losses, 0);
        assert_eq!(carve::to_carve(&written.value), written.value);
        for render in [carve::to_html_with_report, carve::to_markdown_with_report] {
            let result = render(&written.value, CheckedRenderOptions::default()).unwrap();
            assert_eq!(result.total_losses, 1);
            assert_eq!(result.losses[0].code, "destination-denied");
        }
    }
}

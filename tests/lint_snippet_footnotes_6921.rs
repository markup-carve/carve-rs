use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn codes(source: &str, child: Option<&str>) -> Vec<(&'static str, Option<String>)> {
    let options = LintOptions {
        includes: child.map(|text| {
            Includes::new(Arc::new({
                let text = text.to_string();
                move |request: &Request<'_>| {
                    if request.path == "child.crv" {
                        Ok((text.as_bytes().to_vec(), "child.crv".into()))
                    } else {
                        Err(Cause::NotFound)
                    }
                }
            }))
        }),
        ..LintOptions::default()
    };
    lint(source, &options)
        .into_iter()
        .map(|d| (d.code, d.file))
        .collect()
}

#[test]
fn a_thousand_snippet_copy_closures_keep_their_note_used() {
    let mut checked = 0;
    for index in 0..125 {
        let key = format!("note{index}");
        for shape in 0..8 {
            let (source, child) = match shape {
                0 => (
                    format!("{{$s}}: [^{key}]\n\n{{{{ $s }}}}\n\n[^{key}]: Note"),
                    None,
                ),
                1 => (
                    format!("[^{key}]: Note\n\n{{$s}}: [^{key}]\n\n{{{{ $s }}}}"),
                    None,
                ),
                2 => (
                    format!(
                        "{{$s}}: before [^{key}] after\n\n{{{{ $s }}}} and {{{{ $s }}}}\n\n[^{key}]: Note"
                    ),
                    None,
                ),
                3 => (
                    format!(
                        "{{$a}}: [^{key}]\n\n{{$b}}: {{{{ $a }}}}\n\n{{{{ $b }}}}\n\n[^{key}]: Note"
                    ),
                    None,
                ),
                4 => (
                    format!("[^{key}]: Note\n\n{{{{ child.crv }}}}"),
                    Some(format!("{{$s}}: [^{key}]\n\n{{{{ $s }}}}")),
                ),
                5 => (
                    format!("[^{key}]: Note\n\n{{{{ child.crv }}}}"),
                    Some(format!(
                        "{{$a}}: [^{key}]\n\n{{$b}}: {{{{ $a }}}}\n\n{{{{ $b }}}}"
                    )),
                ),
                6 => (
                    format!("[^{key}]: Note\n\n{{{{ $s }}}}\n\n{{{{ child.crv }}}}"),
                    Some(format!("{{$s}}: [^{key}]")),
                ),
                _ => (
                    format!("{{{{ child.crv }}}}\n\n[^{key}]: Note"),
                    Some(format!("{{$s}}: [^{key}]\n\n{{{{ $s }}}}")),
                ),
            };
            let found = codes(&source, child.as_deref());
            assert!(found.is_empty(), "shape={shape} key={key} found={found:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 1_000);
}

#[test]
fn copied_snippet_references_count_in_note_lint() {
    let cases = [
        (
            "{$s}: [^n]\n\n{{ $s }} and {{ $s }}.\n\n[^n]: Note",
            None,
            vec![],
        ),
        (
            "[^n]: Note\n\n{{ child.crv }}",
            Some("{$s}: [^n]\n\n{{ $s }}"),
            vec![],
        ),
        (
            "{$s}: [^n]\n\n{{ $s }}\n\n{{ child.crv }}",
            Some("[^n]: Note"),
            vec![("unused-footnote-definition", Some("child.crv".into()))],
        ),
        (
            "[^n]: Note\n\n{{ child.crv }}",
            Some("{$s}: [^n]"),
            vec![
                ("unused-footnote-definition", None),
                ("unused-snippet", Some("child.crv".into())),
            ],
        ),
        (
            "[^n]: Note\n\n{{ $s }}\n\n{{ child.crv }}",
            Some("{$s}: [^n]"),
            vec![],
        ),
        (
            "[^n]: Root\n\n{{ $s }}\n\n{{ child.crv }}",
            Some("{$s}: [^n]\n\n[^n]: Child"),
            vec![("unused-footnote-definition", None)],
        ),
        (
            "[^n]: Note\n\n{{ child.crv }}",
            Some("{$a}: [^n]\n\n{$b}: {{ $a }}\n\n{{ $b }}"),
            vec![],
        ),
        (
            "[^n]: Root\n\n{{ $b }}\n\n{{ child.crv }}",
            Some("{$a}: [^n]\n\n{$b}: {{ $a }}\n\n[^n]: Child"),
            vec![("unused-footnote-definition", None)],
        ),
    ];
    for (source, child, expected) in cases {
        let got = codes(source, child);
        assert_eq!(got, expected, "{source:?} child={child:?}");
    }
}

#[test]
fn lint_reuses_the_include_content_that_produced_its_findings() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let options = LintOptions {
        includes: Some(Includes::new(Arc::new(move |_request: &Request<'_>| {
            let text = if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                "{$s}: [^n]\n\n{{ $s }}"
            } else {
                "Different content."
            };
            Ok((text.as_bytes().to_vec(), "child.crv".into()))
        }))),
        ..LintOptions::default()
    };
    let found = lint("[^n]: Note\n\n{{ child.crv }}", &options);
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nondefault_heading_ids_keep_copied_note_uses() {
    let options = LintOptions {
        heading_ids: carve::options::HeadingIds::Off,
        includes: Some(Includes::new(Arc::new(|request: &Request<'_>| {
            if request.path == "child.crv" {
                Ok((b"{$s}: [^n]\n\n{{ $s }}".to_vec(), "child.crv".into()))
            } else {
                Err(Cause::NotFound)
            }
        }))),
        ..LintOptions::default()
    };
    let found = lint("[^n]: Note\n\n{{ child.crv }}", &options);
    assert!(found.is_empty(), "{found:?}");
}

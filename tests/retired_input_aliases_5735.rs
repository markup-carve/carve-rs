use carve::{html_to_carve, lint_carve, HtmlImportMode, HtmlImportOptions};

fn imported(input: &str) -> String {
    html_to_carve(
        input,
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..HtmlImportOptions::default()
        },
    )
    .unwrap()
    .value
}

#[test]
fn old_prefix_names_are_ordinary_attributes() {
    for suffix in [
        "ref",
        "src",
        "raw",
        "escaped",
        "autolink",
        "footnote-label",
        "explicit-id",
        "generated-id",
        "id-slot",
        "inline-footnote",
        "col-widths",
        "label",
        "tight",
        "abbreviations",
        "xref",
        "caption-number",
        "div",
        "line-block",
        "substitution",
    ] {
        let name = format!("data-djot-{suffix}");
        let input = format!("<p><span {name}=\"v\">x</span></p>");
        let output = imported(&input);
        assert!(output.contains(&name), "{name}: {output}");
    }
    assert_eq!(
        imported("<p><a href=\"/u\" data-djot-ref=\"r\">t</a></p>"),
        "[t](/u){data-djot-ref=r}\n"
    );
    assert_eq!(
        imported("<p><a href=\"/u\" data-carve-ref=\"r\">t</a></p>"),
        "[t][r]\n\n[r]: /u\n"
    );
}

#[test]
fn old_prefix_variants_remain_unreserved_across_hosts_and_modes() {
    let suffixes = [
        "ref",
        "src",
        "raw",
        "escaped",
        "autolink",
        "footnote-label",
        "explicit-id",
        "generated-id",
        "id-slot",
        "inline-footnote",
        "col-widths",
        "label",
        "tight",
        "abbreviations",
        "xref",
        "caption-number",
        "div",
        "line-block",
        "substitution",
    ];
    let mut checked = 0;
    for suffix in suffixes {
        let name = format!("data-djot-{suffix}");
        for value in ["v", "0", "one-two", "a_b"] {
            for (open, close) in [
                ("<p", "</p>"),
                ("<p><span", "</span></p>"),
                ("<p><a href=\"/u\"", "</a></p>"),
                ("<h2", "</h2>"),
            ] {
                for nested in [false, true] {
                    for mode in [HtmlImportMode::Safe, HtmlImportMode::Roundtrip] {
                        let element = format!("{open} {name}=\"{value}\">x{close}");
                        let html = if nested {
                            format!("<blockquote>{element}</blockquote>")
                        } else {
                            element
                        };
                        let output = html_to_carve(
                            &html,
                            &HtmlImportOptions {
                                mode,
                                ..Default::default()
                            },
                        )
                        .unwrap()
                        .value;
                        assert!(output.contains(&name), "{html}: {output}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1216);
}

#[test]
fn retired_lint_name_does_not_suppress_heading_skip() {
    let ticket = "<!-- carve-lint-disable a11y/heading-jump -->\n# One\n\n### Three\n";
    assert!(lint_carve(ticket)
        .iter()
        .any(|finding| finding.rule == "heading-level-skip"));
    let old = "%% carve-lint-disable a11y/heading-jump\n# One\n\n### Three\n";
    let canonical = old.replace("a11y/heading-jump", "heading-level-skip");
    assert!(lint_carve(old)
        .iter()
        .any(|finding| finding.rule == "heading-level-skip"));
    assert!(!lint_carve(&canonical)
        .iter()
        .any(|finding| finding.rule == "heading-level-skip"));
}

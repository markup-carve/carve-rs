use carve::include::{Cause, Includes, Request};
use carve::lint::{LintOptions, lint};
use std::sync::Arc;

fn options(text: &'static str) -> LintOptions {
    let resolver = Arc::new(move |request: &Request<'_>| {
        if request.path == "a.crv" {
            Ok((text.as_bytes().to_vec(), "a.crv".into()))
        } else {
            Err(Cause::NotFound)
        }
    });
    LintOptions {
        includes: Some(Includes::new(resolver)),
        ..LintOptions::default()
    }
}

#[test]
fn c2_n_085_to_087_lints_accepted_file_once_with_file_position_and_path() {
    let findings = lint("{{ a.crv }}\n\n{{ a.crv }}", &options("**bold**\n\nsafe\n"));
    let hits: Vec<_> = findings
        .iter()
        .filter(|d| d.code == "markdown-strong-double-star")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].file.as_deref(), Some("a.crv"));
    assert_eq!(hits[0].pos.map(|p| p.start_line), Some(1));
    assert_eq!(hits[0].included_by.len(), 1);
    assert_eq!(hits[0].included_by[0].line, 1);
    assert!(
        hits[0]
            .path
            .as_deref()
            .is_some_and(|path| path.starts_with("/children/0/children/"))
    );
}

#[test]
fn c2_n_086_lints_only_the_selected_lines_or_section() {
    let source = "# A\n\n**bold**\n\n# B\n\n**other**\n";
    let opts = options(source);
    let lines = lint("{{ a.crv @lines:1-3 }}", &opts);
    let hits: Vec<_> = lines
        .iter()
        .filter(|d| d.code == "markdown-strong-double-star")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].pos.map(|p| p.start_line), Some(3));
    let section = lint("{{ a.crv#A }}", &opts);
    let hits: Vec<_> = section
        .iter()
        .filter(|d| d.code == "markdown-strong-double-star")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].pos.map(|p| p.start_line), Some(3));
}

#[test]
fn c2_n_085_no_resolver_and_missing_target_are_reported() {
    let no_resolver = lint("{{ a.crv }}", &LintOptions::default());
    assert!(
        no_resolver
            .iter()
            .any(|finding| finding.code == "include-no-resolver")
    );
    let missing = lint("{{ b.crv }}", &options("ready"));
    assert!(
        missing
            .iter()
            .any(|finding| finding.code.starts_with("include-"))
    );
}

/// C2-W-156, C2-B-231: each source's line-1 declaration controls its own notices.
#[test]
fn c2_w_156_each_source_reads_its_own_declaration() {
    let notices = |entry: &str, part: &'static str| {
        lint(entry, &options(part))
            .iter()
            .filter(|d| d.code == "deprecated-indented-line-under-heading")
            .map(|d| d.file.clone())
            .collect::<Vec<_>>()
    };
    assert!(notices("---\ncarve: 0.1\n---\n{{ a.crv }}\n", "# a\n b\n").is_empty());
    assert!(notices("{{ a.crv }}\n", "# a\n b\n").is_empty());
    assert_eq!(
        notices("{{ a.crv }}\n", "---\ncarve: 0.1\n---\n# a\n b\n"),
        [Some("a.crv".to_string())]
    );
}

#[test]
fn c2_w_156_nested_includes_read_their_own_declarations() {
    let resolver = Arc::new(|request: &Request<'_>| match request.path {
        "a.crv" => Ok((
            b"---\ncarve: 0.1\n---\n{{ b.crv }}\n# a\n b\n".to_vec(),
            "a.crv".into(),
        )),
        "b.crv" => Ok((b"# b\n c\n".to_vec(), "b.crv".into())),
        _ => Err(Cause::NotFound),
    });
    let opts = LintOptions {
        includes: Some(Includes::new(resolver)),
        ..LintOptions::default()
    };
    let files: Vec<_> = lint("{{ a.crv }}\n", &opts)
        .into_iter()
        .filter(|d| d.code == "deprecated-indented-line-under-heading")
        .map(|d| d.file)
        .collect();
    assert_eq!(files, [Some("a.crv".into())]);
}

fn paths(source: &str, text: &'static str, code: &str) -> Vec<String> {
    lint(source, &options(text))
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| d.path.unwrap_or_default())
        .collect()
}

#[test]
fn c2_w_138_an_included_finding_keeps_the_path_its_rule_gives() {
    assert_eq!(
        paths(
            "{{ a.crv }}\n",
            "a **b** c\n",
            "markdown-strong-double-star"
        ),
        ["/children/0/children/0"]
    );
    assert_eq!(
        paths(
            "{{ a.crv }}\n",
            "a\n# b\n",
            "block-opener-without-blank-line"
        ),
        ["/children/0/children/0"]
    );
    assert_eq!(
        paths("{{ a.crv }}\n", "> [a]: /u\n", "definition-in-container"),
        ["/children/0/children/0/children/0"]
    );
}

#[test]
fn c2_n_087_a_section_finding_names_its_block_under_the_include() {
    let text = "a ~~b~~ c\n\n= K\n\nd **e** f\n";
    assert_eq!(
        paths("{{ a.crv #K }}\n", text, "markdown-strong-double-star"),
        ["/children/0/children/1"]
    );
}

#[test]
fn c2_n_087_per_column_findings_with_one_range_are_all_kept() {
    let findings = lint(
        "{{ a.crv }}\n",
        &options("{aligns=left,right}\n|={<} a |={>} b |\n"),
    );
    let columns: Vec<_> = findings
        .iter()
        .filter(|d| d.code == "table-column-overlap")
        .map(|d| d.message.split(' ').take(2).collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(columns, ["Column 1", "Column 2"]);
}

#[test]
fn c2_n_086_a_code_payload_finding_names_the_payload() {
    let resolver = Arc::new(move |request: &Request<'_>| {
        if request.path == "a.js" {
            Ok((
                "let \u{202E}a = 1\nlet \u{202E}b = 2\nlet c = 3\n"
                    .as_bytes()
                    .to_vec(),
                "a.js".into(),
            ))
        } else {
            Err(Cause::NotFound)
        }
    });
    let opts = LintOptions {
        includes: Some(Includes::new(resolver)),
        ..LintOptions::default()
    };
    let found: Vec<_> = lint("x\n\n{{ a.js @code @lines:2-3 }}", &opts)
        .into_iter()
        .filter(|d| d.code == "bidi-control-in-source")
        .map(|d| (d.pos.map(|p| p.start_offset), d.path.unwrap_or_default()))
        .collect();
    assert_eq!(found, [(Some(17), "/children/1/children/0".to_string())]);
}

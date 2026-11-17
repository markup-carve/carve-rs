//! Include clauses that neither the corpus nor the reference vectors hold.
use carve2::include::{Cause, Includes, Request};
use carve2::options::{Options, Profile};
use serde_json::Value;
use std::sync::Arc;

fn with(files: &[(&'static str, &'static str)]) -> Options {
    let files: Vec<(&'static str, &'static str)> = files.to_vec();
    let resolver = Arc::new(move |r: &Request<'_>| {
        files
            .iter()
            .find(|(n, _)| *n == r.path)
            .map(|(n, t)| (t.as_bytes().to_vec(), (*n).to_string()))
            .ok_or(Cause::NotFound)
    });
    Options {
        includes: Some(Includes::new(resolver)),
        ..Options::default()
    }
}

fn tree(input: &str, opts: &Options) -> Value {
    serde_json::from_str(&carve2::tree_json(input, opts)).unwrap()
}

#[test]
fn c2_n_020_a_line_selection_is_a_document_positioned_in_its_file() {
    let opts = with(&[("a.crv", "x\n---\na: 1\n---\nbody\n")]);
    let t = tree("{{ a.crv @lines:2-5 }}", &opts);
    let kids = &t["children"][0]["children"];
    assert_eq!(kids[0]["type"], "frontmatter");
    assert_eq!(kids[0]["pos"]["startLine"], 2);
    assert_eq!(kids[1]["children"][0]["pos"]["startOffset"], 15);
    assert_eq!(kids[1]["pos"]["file"], "a.crv");
}

#[test]
fn c2_n_003_no_resolver_reports_each_include() {
    let out = carve2::render_with_diagnostics("{{ a.crv }}\n\n- {{ b.crv }}", &Options::default());
    let codes = out.codes();
    assert_eq!(
        codes.iter().filter(|c| *c == "include-no-resolver").count(),
        2
    );
    assert_eq!(
        codes.iter().filter(|c| *c == "include-unresolved").count(),
        2
    );
}

#[test]
fn c2_n_062_a_file_read_keeps_file_on_a_later_failure() {
    let opts = with(&[("a.crv", "Text.\n")]);
    let t = tree("{{ a.crv #missing }}\n\n{{ b.crv }}", &opts);
    assert_eq!(t["children"][0]["file"], "a.crv");
    assert!(t["children"][0].get("children").is_none());
    assert!(t["children"][1].get("file").is_none());
}

#[test]
fn c2_n_047_cols_names_reorder_the_columns() {
    let opts = with(&[("t.csv", "a,b,c\n1,2,3\n")]);
    let html = carve2::render("{table}\n{{ t.csv @cols:\"c,a\" @code:csv }}", &opts);
    assert!(
        html.contains("<th scope=\"col\">c</th><th scope=\"col\">a</th>"),
        "{html}"
    );
    assert!(html.contains("<td>3</td><td>1</td>"), "{html}");
    let out =
        carve2::render_with_diagnostics("{table}\n{{ t.csv @cols:\"a,a\" @code:csv }}", &opts);
    assert!(out.codes().iter().any(|c| c == "include-cols-duplicate"));
}

#[test]
fn c2_n_055_a_note_is_reached_in_its_own_file_first() {
    let opts = with(&[("a.crv", "In [^n].\n\n[^n]: Inner.\n")]);
    let html = carve2::render("Out [^n].\n\n{{ a.crv }}\n\n[^n]: Outer.", &opts);
    assert!(html.contains("<li id=\"fn1\">\n      <p>Outer."), "{html}");
    assert!(html.contains("<li id=\"fn2\">\n      <p>Inner."), "{html}");
}

#[test]
fn c2_n_077_a_profile_without_include_writes_the_directive() {
    let mut opts = with(&[("a.crv", "Text.\n")]);
    opts.profile = Profile::Comment;
    let out = carve2::render_with_diagnostics("{{ a.crv }}", &opts);
    assert_eq!(out.html, "<p>{{ a.crv }}</p>");
    assert!(
        out.codes()
            .iter()
            .any(|c| c == "profile-construct-degraded")
    );
}

#[test]
fn c2_n_078_c2_n_079_losses_of_expanded_and_unexpanded_includes() {
    let mut opts = with(&[("a.crv", "{{ b.crv }}\n")]);
    opts.roundtrip = true;
    let out = carve2::render_with_diagnostics("{{ a.crv }}", &opts);
    let codes = out.codes();
    assert_eq!(
        codes
            .iter()
            .filter(|c| *c == "include-expanded-roundtrip")
            .count(),
        1
    );
    opts.roundtrip = false;
    opts.strict_losses = true;
    assert!(carve2::try_render("{{ a.crv }}", &opts).is_err());
    let ok = with(&[("a.crv", "A.\n")]);
    let strict = Options {
        strict_losses: true,
        ..ok
    };
    assert_eq!(
        carve2::try_render("{{ a.crv }}", &strict).as_deref(),
        Ok("<p>A.</p>")
    );
}

#[test]
fn c2_n_006_expansion_lists_its_stage() {
    let t = tree("a", &with(&[]));
    assert_eq!(t["stages"][0], "includeExpansion");
    let t = tree("a", &Options::default());
    assert_eq!(t["stages"][0], "referenceResolution");
}

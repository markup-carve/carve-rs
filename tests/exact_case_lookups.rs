//! Every name lookup compares case exactly (`CARVE-P9R-010`): `</#id>`
//! cross-references, collapsed `[Heading][]` references and caption ids. Lint
//! names a case-only miss with the exact spelling, and `fmt --migrate` rewrites
//! a reference whose one case-insensitive match is spelled differently.

use std::io::Write;
use std::process::{Command, Stdio};

use carve::{lint_carve, migrate_case_only_references, to_html, Options};

fn html(src: &str) -> String {
    to_html(src).trim().to_string()
}

fn migrate(src: &str) -> String {
    migrate_case_only_references(src, &Options::default())
}

fn messages(src: &str, rule: &str) -> Vec<String> {
    lint_carve(src)
        .into_iter()
        .filter(|w| w.rule == rule)
        .map(|w| w.message)
        .collect()
}

#[test]
fn a_crossref_that_differs_only_in_case_stays_literal() {
    assert_eq!(
        html("# Getting Started\n\nSee </#getting-started>.\n"),
        "<section id=\"Getting-Started\">\n  <h1>Getting Started</h1>\n  <p>See &lt;/#getting-started&gt;.</p>\n</section>"
    );
}

#[test]
fn ids_that_differ_only_in_case_are_two_targets() {
    let out = html("{#Tip}\n# Upper\n\n{#tip}\n# Lower\n\n</#Tip> and </#tip>\n");
    assert!(
        out.contains("<a href=\"#Tip\">Upper</a> and <a href=\"#tip\">Lower</a>"),
        "{out}"
    );
}

#[test]
fn a_caption_crossref_compares_case_exactly() {
    let src =
        "{#Fig-A}\n![A sunset](sun.jpg)\n^ Figure #: A sunset\n\nSee </#fig-a> and </#Fig-A>.\n";
    let out = html(src);
    assert!(
        out.contains("See &lt;/#fig-a&gt; and <a href=\"#Fig-A\">"),
        "{out}"
    );
}

#[test]
fn a_numbered_crossref_compares_case_exactly() {
    let ext = carve::HeadingNumbers::new();
    let opts = Options::new().with_extension(&ext);
    let out = carve::to_html_with_options(
        "# Some Title\n\nSee </#some-title> and </#Some-Title>.\n",
        &opts,
    );
    assert!(
        out.contains("See &lt;/#some-title&gt; and <a href=\"#Some-Title\">"),
        "{out}"
    );
}

#[test]
fn a_collapsed_reference_compares_heading_text_case_exactly() {
    let out = html("See [Plan][] and [plan][].\n\n# Plan\n");
    assert!(
        out.contains("<p>See <a href=\"#Plan\">Plan</a> and [plan][].</p>"),
        "{out}"
    );
}

#[test]
fn a_collapsed_reference_still_collapses_whitespace() {
    let out = html("See [Getting   Started][].\n\n# Getting Started\n");
    assert!(out.contains("href=\"#Getting-Started\""), "{out}");
}

#[test]
fn lint_names_the_exact_id_of_a_case_only_crossref_miss() {
    assert_eq!(
        messages("# Plan\n\nSee </#plan>.\n", "broken-crossref"),
        ["Cross-reference </#plan> matches no id; the id \"Plan\" differs only in case, and cross-references are case-sensitive, so it renders as the literal text \"</#plan>\"."]
    );
}

#[test]
fn lint_names_every_id_a_crossref_misses_only_by_case() {
    let found = messages("{#Tip}\n# A\n\n{#tip}\n# B\n\n</#TIP>\n", "broken-crossref");
    assert_eq!(found.len(), 1);
    assert!(
        found[0].contains("the ids \"Tip\" and \"tip\" differ only in case"),
        "{found:?}"
    );
}

#[test]
fn lint_names_a_caption_id_missed_only_by_case() {
    let found = messages(
        "{#Fig-A}\n![A sunset](sun.jpg)\n^ Figure #: A sunset\n\nSee </#fig-a>.\n",
        "broken-crossref",
    );
    assert_eq!(found.len(), 1);
    assert!(
        found[0].contains("the id \"Fig-A\" differs only in case"),
        "{found:?}"
    );
}

#[test]
fn lint_keeps_the_plain_message_when_nothing_matches_by_case() {
    assert_eq!(
        messages("# Plan\n\nSee </#nope>.\n", "broken-crossref"),
        ["Cross-reference </#nope> has no matching heading id."]
    );
    assert_eq!(
        messages("See [x][nope].\n", "unresolved-reference-link"),
        ["Reference link has no matching definition or heading."]
    );
}

#[test]
fn lint_names_the_exact_label_of_a_case_only_reference_miss() {
    assert_eq!(
        messages("[x][Label]\n\n[label]: /u\n", "unresolved-reference-link"),
        ["Reference link [x][Label] matches no definition or heading; the label \"label\" differs only in case, and reference labels are case-sensitive, so it renders as literal text."]
    );
}

#[test]
fn lint_names_the_exact_heading_of_a_case_only_collapsed_miss() {
    let found = messages(
        "See [getting started][].\n\n# Getting Started\n",
        "unresolved-reference-link",
    );
    assert_eq!(found.len(), 1);
    assert!(
        found[0].contains("the heading \"Getting Started\" differs only in case"),
        "{found:?}"
    );
}

#[test]
fn an_explicit_label_is_not_matched_against_headings_even_by_case() {
    // PART 9R R1: only the collapsed form reaches the heading index.
    assert_eq!(
        messages("See [x][plan].\n\n# Plan\n", "unresolved-reference-link"),
        ["Reference link has no matching definition or heading."]
    );
}

#[test]
fn migrate_rewrites_each_reference_with_one_case_only_match() {
    let src = "# Getting Started\n\nSee </#getting-started>, [getting started][] and [x][Label].\n\n[label]: /u\n";
    assert_eq!(
        migrate(src),
        "# Getting Started\n\nSee </#Getting-Started>, [Getting Started][] and [x][label].\n\n[label]: /u\n"
    );
}

#[test]
fn migrate_also_collapses_the_whitespace_lookup_ignores() {
    assert_eq!(
        migrate(
            "# Getting Started\n\n[getting   started][] and [x][MY   LABEL]\n\n[My Label]: /u\n"
        ),
        "# Getting Started\n\n[Getting Started][] and [x][My Label]\n\n[My Label]: /u\n"
    );
}

#[test]
fn migrate_keeps_a_trailing_attribute_block() {
    assert_eq!(
        migrate("# Plan\n\n[plan][]{.a} and [x][LABEL]{.b}\n\n[Label]: /u\n"),
        "# Plan\n\n[Plan][]{.a} and [x][Label]{.b}\n\n[Label]: /u\n"
    );
}

#[test]
fn migrate_accepts_a_canonically_equivalent_spelling() {
    assert_eq!(
        migrate("# \u{c9}cole\n\n[e\u{301}cole][]\n"),
        "# \u{c9}cole\n\n[\u{c9}cole][]\n"
    );
}

#[test]
fn migrate_leaves_a_reference_with_several_case_only_matches() {
    let src = "{#Tip}\n# A\n\n{#tip}\n# B\n\n</#TIP>\n";
    assert_eq!(migrate(src), src);
}

#[test]
fn migrate_leaves_resolved_and_unmatched_references_alone() {
    let src = "# Plan\n\nSee </#Plan>, </#nope>, [Plan][] and [x][nope].\n";
    assert_eq!(migrate(src), src);
}

#[test]
fn migrate_is_idempotent_and_the_result_resolves() {
    let once = migrate("# Ärger\n\nSiehe </#ärger> und [ärger][].\n");
    assert_eq!(once, "# Ärger\n\nSiehe </#Ärger> und [Ärger][].\n");
    assert_eq!(migrate(&once), once);
    let out = html(&once);
    assert!(!out.contains("&lt;/#"), "{out}");
    assert!(!out.contains("[Ärger][]"), "{out}");
    assert!(messages(&once, "broken-crossref").is_empty());
}

#[test]
fn migrate_reaches_into_containers_and_footnote_bodies() {
    let src = "# H\n\n> See </#h>.\n\nBody[^n]\n\n[^n]: see </#h>\n";
    assert_eq!(
        migrate(src),
        "# H\n\n> See </#H>.\n\nBody[^n]\n\n[^n]: see </#H>\n"
    );
}

#[test]
fn fmt_migrate_rewrites_before_formatting() {
    let run = |args: &[&str]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
            .arg("fmt")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("carve fmt runs");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"# Plan\n\nSee </#plan>.\n")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    assert_eq!(run(&["--migrate"]), "# Plan\n\nSee </#Plan>.\n");
    assert_eq!(run(&[]), "# Plan\n\nSee </#plan>.\n");
}

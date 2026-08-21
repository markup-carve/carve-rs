//! Optional Tier-2 corpus tests, driven from `manifest.json`.
//!
//! The manifest is the POPULATION. A case it gains upstream is a case this file
//! runs, or a case this file fails on - never a case that quietly does not
//! exist here. Before carve-rs#1188 the runners were hand-written, one `#[test]`
//! per slug: 22 of the manifest's 45 cases had a test and the other 23 had
//! nothing at all, not even a skip somebody could count, and the file still
//! reported a clean pass.
//!
//! Four guards keep that from coming back, and each one can fail:
//!
//!  - a case whose feature has no runner fails, unless `DECLARED_UNIMPLEMENTED`
//!    says why this engine cannot do it;
//!  - a `DECLARED_UNIMPLEMENTED` entry whose extension this build registers
//!    fails - the excuse is checked, not taken;
//!  - a declaration naming a feature or slug the manifest does not state fails,
//!    because it asserts nothing and reads as coverage;
//!  - and the run reconciles what it REACHED against what it COMPARED as an
//!    IDENTITY, not a floor, so a case that fell through a `continue` is the
//!    difference between two numbers rather than an absence nobody can see.

use std::fs;
use std::path::PathBuf;

use carve::extensions::SemanticSpan;
use carve::{
    Autolink, Citations, CodeCallouts, Details, ListTable, Options, SmartQuotes,
    SmartTypographyMode, Spoiler, Tabs,
};

/// The render target a case names, defaulting to HTML (carve#360). The
/// extension is part of the PAIRING RULE, not a label: an expected file is
/// located from the slug and the target alone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Html,
    Markdown,
    Plain,
    Ansi,
}

impl Target {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "html" => Some(Target::Html),
            "markdown" => Some(Target::Markdown),
            "plain" => Some(Target::Plain),
            "ansi" => Some(Target::Ansi),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Target::Html => "html",
            Target::Markdown => "md",
            Target::Plain => "txt",
            Target::Ansi => "ansi",
        }
    }

    fn render(self, source: &str, options: &Options<'_>) -> String {
        match self {
            Target::Html => carve::to_html_with_options(source, options),
            Target::Markdown => carve::to_markdown_with_options(source, options),
            Target::Plain => carve::to_plain_text_with_options(source, options),
            Target::Ansi => carve::to_ansi_with_options(source, options),
        }
    }
}

/// Features this engine genuinely does not implement, each with the reason and
/// the registry key that would prove it wrong.
///
/// A skip listed here is a statement about the ENGINE. A skip NOT listed here
/// would be a statement about this FILE, and fails instead. Empty is the
/// correct state: an entry silences a comparison whether or not the engine
/// would have passed it, so one goes in only with the reason it cannot be a
/// runner instead.
///
/// The third column is the extension registry key the feature would need, or
/// `None` for a feature that is a render option and so registers nothing. It is
/// what `declared_unimplemented_is_still_true` ratchets on.
const DECLARED_UNIMPLEMENTED: &[(&str, &str, Option<&str>)] = &[];

/// Cases this engine has DELIBERATELY moved PAST the pinned corpus on - the
/// same window `tests/corpus.rs` keeps for the core corpus.
///
/// Each entry FAILS IN BOTH DIRECTIONS: the output must equal what this engine
/// now states, so a regression is caught exactly as the corpus would have
/// caught it, and it must still DIFFER from the pinned fixture, so an entry the
/// pin has caught up on fails and is deleted in the commit that moves the pin.
const AHEAD_OF_PIN: &[(&str, &str, &str)] = &[(
    "28-tabs-panel-title",
    "markup-carve/carve#1468 names the tab set as a whole and extensions §13.2 \
names each css-mode PANEL after its own tab; the pinned corpus predates both. \
markup-carve/carve#1477 updates the fixture, and this entry goes in the commit \
that moves the pin.",
    r#"<div class="tabs" role="group" aria-label="Tabs">
<input type="radio" name="tabset-1" id="tabset-1-tab-1" class="tabs-radio" checked>
<label for="tabset-1-tab-1" class="tabs-label">First</label>
<div class="tabs-panel" role="group" aria-label="First">
<p class="admonition-title">Inner <strong>Title</strong></p>
<p>Content one.</p>
</div>
</div>
"#,
)];

/// The floor a manifest emptied or halved cannot get past. It sits under the
/// count today for the same reason the other floors in this repo do: the
/// optional corpus is append-only, so a number below it can only be reached by
/// loss. The floor alone cannot see a case the loop reached and dropped - that
/// is what the reconciliation identity is for.
const COMPARED_FLOOR: usize = 35;

fn optional_corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus-optional")
}

struct Case {
    slug: String,
    feature: String,
    target: Target,
}

fn manifest_cases() -> Vec<Case> {
    let path = optional_corpus_dir().join("manifest.json");
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e}\nDid you initialize and update the `tests/spec` submodule?",
            path.display()
        )
    });
    let manifest: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let cases = manifest["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("{}: `cases` is not an array", path.display()));
    cases
        .iter()
        .map(|case| {
            let slug = case["slug"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: a case has no `slug`", path.display()))
                .to_string();
            let feature = case["feature"]
                .as_str()
                .unwrap_or_else(|| panic!("{slug}: no `feature`"))
                .to_string();
            let target_name = case["target"].as_str().unwrap_or("html");
            let target = Target::parse(target_name)
                .unwrap_or_else(|| panic!("{slug}: unknown target `{target_name}`"));
            Case {
                slug,
                feature,
                target,
            }
        })
        .collect()
}

fn read_pair(slug: &str, target: Target) -> (String, String) {
    let dir = optional_corpus_dir();
    let crv = dir.join(format!("{slug}.crv"));
    let expected_path = dir.join(format!("{slug}.{}", target.extension()));
    let source = fs::read_to_string(&crv).unwrap_or_else(|e| panic!("read {}: {e}", crv.display()));
    let expected = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display()));
    (source, expected)
}

/// Supply a feature's configuration and render through the target the case
/// named, so one arm serves a feature pinned on more than one target.
///
/// The configurations are the ones the spec's own runner uses
/// (`tests/spec/tests/optional-corpus.test.mjs`), deliberately spelled the same
/// way: a feature id means one thing, and two files disagreeing about what it
/// configures is a divergence nothing would report.
///
/// `None` means no runner - the caller decides whether that is a declared gap
/// or a failure.
fn render_feature(feature: &str, source: &str, target: Target) -> Option<String> {
    let citations = Citations::new();
    let citations_author_date = Citations::author_date();
    let list_table = ListTable::new();
    let autolink = Autolink::new();
    let code_callouts = CodeCallouts::new();
    let details = Details::new();
    let spoiler = Spoiler::new();
    let tabs = Tabs::new();
    let semantic_span = SemanticSpan;
    let smart_quotes_de = SmartQuotes::new("de");

    // Carve-rs spells the typography switch `glyph | source` and has no third
    // "off" state: with no glyph substitution to make, `Source` renders the
    // author's run, which is what an engine with an off switch writes. The
    // manifest's `smart-typography-off` case is compared against that.
    let mut source_typography = Options::new();
    source_typography.smart_typography = SmartTypographyMode::Source;

    let output = match feature {
        "social-link-templates" => target.render(
            source,
            &Options::new()
                .with_mention_url("/users/{name}")
                .with_tag_url("/topics/{name}"),
        ),
        "symbol-map" => target.render(
            source,
            &Options::new()
                .with_symbol("rocket", "\u{1F680}")
                .with_symbol("tada", "\u{1F389}")
                .with_symbol("+1", "\u{1F44D}")
                .with_symbol("UPPER", "\u{2B06}\u{FE0F}"),
        ),
        "smart-quotes-locale-de" => {
            target.render(source, &Options::new().with_extension(&smart_quotes_de))
        }
        "bare-url-autolink" => target.render(source, &Options::new().with_extension(&autolink)),
        "citations-numbered" => target.render(source, &Options::new().with_extension(&citations)),
        "citations-author-date" => target.render(
            source,
            &Options::new().with_extension(&citations_author_date),
        ),
        "code-callouts" => target.render(source, &Options::new().with_extension(&code_callouts)),
        "details" => target.render(source, &Options::new().with_extension(&details)),
        "spoiler" => target.render(source, &Options::new().with_extension(&spoiler)),
        "tabs" => target.render(source, &Options::new().with_extension(&tabs)),
        "semantic-span" => target.render(source, &Options::new().with_extension(&semantic_span)),
        "list-table" | "list-table-columns-1344" | "list-table-local-headers-1248" => {
            target.render(source, &Options::new().with_extension(&list_table))
        }
        // The switch, on each target that pins it. Three feature ids rather than
        // one shared id, because a manifest entry names one feature and one
        // target, and an engine that carries the mode on Markdown but drops it
        // on plain text has to be able to say so (carve#560).
        "smart-typography-off"
        | "markdown-typography-source"
        | "plain-typography-source"
        | "ansi-typography-source" => target.render(source, &source_typography),
        // DEFAULT typography, with no switch at all: the control for the
        // source-mode cases. Without one, a case pinning the source spelling
        // also passes an engine that never applies typography to that construct
        // in either mode (carve#915).
        "smart-typography-default" => target.render(source, &Options::new()),
        "section-wrapper-off" => target.render(source, &Options::new().with_sections(false)),
        "source-line-after-generated-id" => target.render(
            source,
            &Options::new().with_sections(false).with_source_lines(true),
        ),
        _ => return None,
    };
    Some(output)
}

#[test]
fn every_optional_corpus_case_is_compared_or_declared() {
    let cases = manifest_cases();
    let mut reached = 0usize;
    let mut compared = 0usize;
    let mut declared = 0usize;
    let mut ahead = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for case in &cases {
        reached += 1;
        let slug = case.slug.rsplit('/').next().unwrap_or(&case.slug);

        let Some(actual) =
            render_feature(&case.feature, &read_pair(slug, case.target).0, case.target)
        else {
            match DECLARED_UNIMPLEMENTED
                .iter()
                .find(|(feature, _, _)| *feature == case.feature)
            {
                Some((_, reason, _)) => {
                    declared += 1;
                    eprintln!("skipped {slug} ({}): {reason}", case.feature);
                }
                None => failures.push(format!(
                    "{slug}: no runner for feature `{}` and no DECLARED_UNIMPLEMENTED entry. \
                     Either write the runner, or say why this engine cannot do it - an \
                     undeclared skip reads as coverage.",
                    case.feature
                )),
            }
            continue;
        };

        let (_, expected) = read_pair(slug, case.target);

        if let Some((_, reason, stated)) = AHEAD_OF_PIN.iter().find(|(name, _, _)| *name == slug) {
            ahead += 1;
            if actual.trim() != stated.trim() {
                failures.push(format!(
                    "{slug} ({reason}) did not match what this engine states.\n\
                     ----- stated -----\n{}\n----- actual -----\n{}\n------------------",
                    stated.trim(),
                    actual.trim()
                ));
            }
            // The staleness half: when the pin moves past this rule the fixture
            // is rewritten to exactly this value, and the entry must go.
            if expected.trim() == stated.trim() {
                failures.push(format!(
                    "{slug} now matches the pinned corpus: delete its AHEAD_OF_PIN entry"
                ));
            }
            continue;
        }

        compared += 1;
        if expected.trim() != actual.trim() {
            failures.push(format!(
                "optional corpus pair `{slug}` ({}, {:?}) did not match.\n\
                 ----- expected -----\n{}\n----- actual -------\n{}\n--------------------",
                case.feature,
                case.target,
                expected.trim(),
                actual.trim()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {reached} optional corpus case(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );

    // A runner generated from a manifest reports a clean run when the manifest
    // is empty, because zero tests pass.
    assert!(
        compared >= COMPARED_FLOOR,
        "only {compared} case(s) compared; tests/spec/tests/corpus-optional/manifest.json is \
         the population, and a run over fewer of it registers fewer assertions and still \
         exits 0",
    );

    // And a floor cannot see a case the loop REACHED and dropped, which is the
    // hole carve-rs#1188 came through. Stated as an identity, not a floor, so
    // the two sides cannot drift.
    assert_eq!(
        compared + declared + ahead,
        reached,
        "{reached} case(s) reached, but {compared} compared + {declared} declared \
         unimplemented + {ahead} ahead of the pin - the difference is cases nobody checked",
    );
}

/// The ratchet on the excuse. A `DECLARED_UNIMPLEMENTED` entry can only ever
/// turn a comparison into a skip, and the condition it carries is checkable:
/// a feature whose extension this build REGISTERS is implemented, whatever the
/// table says. A feature that is a render option registers nothing and states
/// `None` - correct, because an option's absence is not something the registry
/// can report.
#[test]
fn declared_unimplemented_is_still_true() {
    let stale: Vec<&str> = DECLARED_UNIMPLEMENTED
        .iter()
        .filter(|(_, _, key)| {
            key.is_some_and(|key| carve::extensions::registry::by_key(key).is_some())
        })
        .map(|(feature, _, _)| *feature)
        .collect();
    assert!(
        stale.is_empty(),
        "this build registers the extension these feature(s) need: {stale:?} - write the \
         runner and delete the DECLARED_UNIMPLEMENTED entry",
    );
}

#[test]
fn declared_unimplemented_names_only_features_the_manifest_states() {
    let stated: Vec<String> = manifest_cases().into_iter().map(|c| c.feature).collect();
    let orphaned: Vec<&str> = DECLARED_UNIMPLEMENTED
        .iter()
        .map(|(feature, _, _)| *feature)
        .filter(|feature| !stated.iter().any(|s| s == feature))
        .collect();
    assert!(
        orphaned.is_empty(),
        "DECLARED_UNIMPLEMENTED names feature(s) the manifest does not state: {orphaned:?} - \
         renamed upstream, or already retired; either way the entry excuses nothing",
    );
}

#[test]
fn ahead_of_pin_names_only_cases_the_manifest_states() {
    let stated: Vec<String> = manifest_cases()
        .into_iter()
        .map(|c| c.slug.rsplit('/').next().unwrap_or(&c.slug).to_string())
        .collect();
    let orphaned: Vec<&str> = AHEAD_OF_PIN
        .iter()
        .map(|(slug, _, _)| *slug)
        .filter(|slug| !stated.iter().any(|s| s == slug))
        .collect();
    assert!(
        orphaned.is_empty(),
        "AHEAD_OF_PIN names case(s) the manifest does not state: {orphaned:?}",
    );
}

/// Not a corpus pair: a configured template that resolves to a dangerous scheme
/// must be sanitized at the href, not merely at the name.
#[test]
fn social_link_templates_sanitize_final_href() {
    let options = Options::new()
        .with_mention_url("javascript:alert({name})")
        .with_tag_url("javascript:alert({name})");
    let html = carve::to_html_with_options("@alice #topic", &options);
    assert!(
        html.contains("<a class=\"mention\" href=\"\">@alice</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a class=\"tag\" href=\"\">#topic</a>"),
        "{html}"
    );
}

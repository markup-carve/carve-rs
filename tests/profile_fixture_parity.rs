//! The shared profile battery, `tests/spec/tests/profile-fixtures.json`.
//!
//! That file names carve-php as the reference and says the other engines assert
//! against it. Nothing in this engine had ever read it, which is part of how all
//! three drifted into the same defect unseen: a profile denied any node type its
//! vocabulary did not list, so a construct the vocabulary predates rendered as
//! NOTHING - not degraded to text, gone (carve#419).
//!
//! The fixtures carry a trailing newline because carve-php's `convert()` emits
//! one; `to_html_with_options` does not. That is an API difference, not a
//! rendering one, so it is normalized rather than papered over in either engine.

use carve::{to_html_with_options, Options, Profile};

fn profile_named(name: &str) -> Profile {
    match name {
        "full" => Profile::full(),
        "article" => Profile::article(),
        "comment" => Profile::comment(),
        "minimal" => Profile::minimal(),
        other => panic!("unknown profile in the battery: {other}"),
    }
}

fn render(source: &str, profile: Profile) -> String {
    let options = Options::default().with_profile(profile);
    to_html_with_options(source, &options)
}

/// Minimal extraction of the fields this battery uses, so the test needs no
/// JSON dependency the crate does not already have.
fn field(entry: &str, key: &str) -> String {
    let needle = format!("\"{key}\":");
    let start = entry
        .find(&needle)
        .unwrap_or_else(|| panic!("no {key} in {entry}"))
        + needle.len();
    let rest = entry[start..].trim_start();
    assert!(rest.starts_with('"'), "{key} is not a string in {entry}");
    let bytes = rest.as_bytes();
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => break,
            b'\\' => {
                i += 1;
                match bytes[i] {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'u' => {
                        let hex = &rest[i + 1..i + 5];
                        let cp = u32::from_str_radix(hex, 16).expect("a \\u escape");
                        out.push(char::from_u32(cp).expect("a scalar value"));
                        i += 4;
                    }
                    other => panic!("unhandled escape \\{}", other as char),
                }
            }
            _ => {
                let ch = rest[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8() - 1;
            }
        }
        i += 1;
    }
    out
}

/// Every fixture entry, as `(name, entry-slice)`.
///
/// No brace matching: a fixture's `carve` value contains `{=html}`, and every
/// scheme that tracked depth got that wrong in a different way. Each entry is
/// delimited by its own `"carve":` key instead, which cannot appear inside a
/// value, and the name is the quoted string before the object that holds it.
fn fixtures(raw: &str) -> Vec<(String, &str)> {
    let mut out = Vec::new();
    let starts: Vec<usize> = raw.match_indices("\"carve\":").map(|(i, _)| i).collect();
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(raw.len());
        let name = raw[..start]
            .rfind('{')
            .and_then(|brace| {
                let head = &raw[..brace];
                let close = head.rfind('"')?;
                let open = head[..close].rfind('"')?;
                Some(head[open + 1..close].to_string())
            })
            .unwrap_or_else(|| format!("fixture {n}"));
        out.push((name, &raw[start..end]));
    }
    out
}

#[test]
fn every_fixture_matches() {
    let raw = std::fs::read_to_string("tests/spec/tests/profile-fixtures.json")
        .expect("the spec submodule must be checked out");
    let cases = fixtures(&raw);
    assert!(
        cases.len() >= 14,
        "expected the full battery, parsed {}",
        cases.len()
    );

    let mut failures = Vec::new();
    for (name, entry) in &cases {
        let source = field(entry, "carve");
        let expected = field(entry, "html");
        let profile = field(entry, "profile");
        let got = render(&source, profile_named(&profile));
        if got.trim_end_matches('\n') != expected.trim_end_matches('\n') {
            failures.push(format!("{name}: got {got:?}, fixture says {expected:?}"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_profile_that_denies_nothing_is_lossless() {
    // The property behind the battery's `full-*` cases, stated directly: it has
    // to hold for every construct, not only the ones someone thought to add.
    let constructs = [
        ("substitution", "a {~old~>new~} b\n"),
        ("symbol", "a :smile: b\n"),
        ("smart quotes", "a \"quoted\" b\n"),
        ("dashes", "a -- b --- c\n"),
        ("critic insert", "a {++ins++} b\n"),
        ("critic delete", "a {--del--} b\n"),
        ("highlight", "a {=mark=} b\n"),
        ("cross reference", "# Title {#t}\n\nSee [here](#t).\n"),
        ("table", "| a | b |\n|---|---|\n| c | d |\n"),
        ("definition list", ":: term\n:  definition\n"),
        ("footnote", "a[^r]\n\n[^r]: note\n"),
        ("admonition", "::: note\nbody\n:::\n"),
        ("abbreviation", "*[HTML]: HyperText\n\nHTML is fine.\n"),
    ];

    // NOT covered here: an EMPTY link (`[](#t)`) disappears under any profile,
    // because the profile path runs an empty-container cleanup the unfiltered
    // render does not. That is a different defect with the same symptom, open
    // in carve-php as a pinned ratchet entry, and it is not what this change
    // is about - noted so a later reader does not think it was missed.
    for (label, source) in constructs {
        assert_eq!(
            render(source, Profile::full()),
            carve::to_html(source),
            "full() changed the output of {label}"
        );
    }
}

#[test]
fn a_disallowed_substitution_keeps_both_texts() {
    // `to_text` promises the words survive and the markup does not. This
    // engine kept only the NEW text, silently dropping the wording the author
    // replaced - and carve-php and carve-js keep both.
    let html = render(
        "Body with a {~old~>new~} substitution.\n",
        Profile::comment(),
    );

    assert!(html.contains("oldnew"), "got {html:?}");
    assert!(!html.contains("<del>"), "got {html:?}");
}

use super::{escape_plain_carve_syntax, HandledDelimiters};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The profiles THIS crate can produce, by the corpus's names.
///
/// The corpus tells an engine to run "every profile its converters can
/// produce" and skip the rest, the way the render corpus skips a target an
/// engine does not implement. All three are listed here deliberately: the
/// handled set is a parameter now, so a profile with no caller is still a
/// statement this implementation can be held to, and `markdown` and `plain`
/// are exactly what a future text-level converter would pass. Which of them
/// has a caller today is recorded in `a_profile_with_no_caller_is_named`.
fn profiles() -> BTreeMap<&'static str, HandledDelimiters<'static>> {
    BTreeMap::from([
        ("plain", HandledDelimiters::PLAIN),
        ("markdown", HandledDelimiters::MARKDOWN),
        ("djot", HandledDelimiters::DJOT),
    ])
}

fn corpus() -> serde_json::Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus-escape/cases.json");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "escaper corpus not found at {}: {e}\n\
             Did you initialize the submodule?\n  git submodule update --init",
            path.display()
        )
    });
    serde_json::from_str(&raw).expect("escaper corpus is JSON")
}

#[test]
fn the_handled_sets_match_the_corpus_profiles() {
    // The sets are spelled in two places - here and in the corpus - and a
    // silent drift between them would make every case below pass while
    // measuring the wrong question.
    let corpus = corpus();
    let declared = corpus["profiles"]
        .as_object()
        .expect("profiles is an object");
    for (name, handled) in profiles() {
        let entry = declared
            .get(name)
            .unwrap_or_else(|| panic!("corpus declares no profile {name}"));
        assert_eq!(
            entry.get("braced").and_then(|v| v.as_str()).unwrap_or(""),
            handled.braced,
            "{name}: braced handled set"
        );
        assert_eq!(
            entry.get("bare").and_then(|v| v.as_str()).unwrap_or(""),
            handled.bare,
            "{name}: bare handled set"
        );
    }
}

#[test]
fn a_case_is_read() {
    // Guards the sweep below against a glob that quietly matches nothing -
    // a checker reporting success having compared nothing is the state this
    // rule was already in.
    let corpus = corpus();
    let cases = corpus["cases"].as_array().expect("cases is an array");
    assert!(cases.len() >= 50, "found {} cases", cases.len());
}

#[test]
fn every_case_matches_under_every_profile() {
    let corpus = corpus();
    let profiles = profiles();
    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for case in corpus["cases"].as_array().expect("cases is an array") {
        let name = case["name"].as_str().expect("a case has a name");
        let input = case["input"].as_str().expect("a case has an input");
        for (profile, expected) in case["expected"]
            .as_object()
            .expect("a case has expectations")
        {
            let Some(handled) = profiles.get(profile.as_str()) else {
                continue;
            };
            let expected = expected.as_str().expect("an expectation is a string");
            let got = escape_plain_carve_syntax(input, *handled);
            if got != expected {
                failures.push(format!(
                    "{name} [{profile}]: {input:?} -> {got:?}, expected {expected:?}"
                ));
            }
            checked += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} escaper cases diverge:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked >= 150, "checked only {checked} case-profile pairs");
}

#[test]
fn escaping_only_ever_inserts_backslashes() {
    // The corpus's own invariant, restated against THIS implementation: an
    // expectation is a fabrication if removing its backslashes does not give
    // the input back. Asserting it on the output rather than on the fixture
    // catches an escaper that rewrites text instead of freezing it.
    let corpus = corpus();
    for case in corpus["cases"].as_array().expect("cases is an array") {
        let input = case["input"].as_str().expect("a case has an input");
        for (profile, handled) in profiles() {
            let got = escape_plain_carve_syntax(input, handled);
            assert_eq!(
                got.replace('\\', ""),
                *input,
                "{} [{profile}] rewrote its input",
                case["name"]
            );
        }
    }
}

#[test]
fn a_delimiter_with_a_space_against_it_is_where_the_opener_test_bites() {
    // `braced_run_opens` is the test that separates "does not open" from
    // "opens and is never closed", and the corpus does NOT reach it: its
    // only inner-space case is `a { ^x^ } b`, whose space sits between the
    // `{` and the delimiter, so the loop never matches the opener at all and
    // every answer this test could give passes. The case that reaches it has
    // the space AFTER the delimiter, and it is pinned here instead.
    assert_eq!(
        escape_plain_carve_syntax("a { ^x^ } b", HandledDelimiters::PLAIN),
        "a { ^x^ } b"
    );
    assert_eq!(
        escape_plain_carve_syntax("a {^ x^} b", HandledDelimiters::PLAIN),
        "a {^ x^} b"
    );
    assert_eq!(
        escape_plain_carve_syntax("a {^ x b", HandledDelimiters::PLAIN),
        "a {^ x b"
    );

    // STATED, NOT ASSERTED AS CORRECT. `a {^ x^} b` renders `a <sup> x</sup>
    // b` in Carve, so the second line above is a LEAK: text that is literal
    // in the calling language reaches Carve as markup. The same holds on the
    // Djot path for the delimiters Djot does not own - `a {, x,} b` is
    // literal Djot and renders `a <sub> x</sub> b` as Carve. Widening the
    // opener to accept a space against the delimiter fixes it, and is left
    // alone here on purpose: the shared corpus has no case for it, carve-php
    // passes the same corpus with the same boundary, and moving one engine
    // off an unpinned shape is how the four spellings drifted apart in the
    // first place. It wants a corpus case and all three engines, not a
    // unilateral change under a ticket about exposing the function.
}

#[test]
fn the_profiles_remain_distinct() {
    // Djot and BBCode are text-level converters. Markdown and HTML build an
    // AST and let the canonical writer emit source, so they escape no text
    // and Markdown passes no handled set.
    // The handled set is what separates the profiles: `*` is Djot markup and
    // is left for the converter, and is literal text under PLAIN. Asserting
    // the two differ is what keeps the parameter load-bearing - a hardwired
    // set would make every profile give the same answer and every case above
    // would still pass.
    assert_eq!(
        escape_plain_carve_syntax("a *x* b", HandledDelimiters::DJOT),
        "a *x* b"
    );
    assert_eq!(
        escape_plain_carve_syntax("a *x* b", HandledDelimiters::PLAIN),
        "a \\*x* b"
    );
    assert_eq!(
        escape_plain_carve_syntax("a ~x~ b", HandledDelimiters::MARKDOWN),
        "a ~x~ b"
    );
    assert_eq!(
        escape_plain_carve_syntax("a ~x~ b", HandledDelimiters::PLAIN),
        "a \\~x~ b"
    );
}

/// An at-sign that opens a Carve mention is escaped when it arrives as
/// text. The sibling of the tag rule, ported from carve-php#1381.
#[test]
fn an_at_sign_in_source_text_is_not_a_mention() {
    for (input, want) in [
        ("hi @user ok", "hi \\@user ok"),
        ("@click toggles it", "\\@click toggles it"),
        ("use @keydown.window here", "use \\@keydown.window here"),
        ("see (@can) there", "see (\\@can) there"),
        ("the @-form", "the \\@-form"),
        ("@can and @click", "\\@can and \\@click"),
    ] {
        assert_eq!(
            escape_plain_carve_syntax(input, HandledDelimiters::DJOT),
            want,
            "input {input:?}"
        );
    }
}

/// BOUND: the escape mirrors the parser's opener, so an at-sign the parser
/// never opens on gains no backslash.
#[test]
fn an_at_sign_that_opens_nothing_is_left_bare() {
    for input in [
        "mail me at foo@bar.de",
        "a@b",
        "name @ handle",
        "ping @, later",
        "ends with @",
    ] {
        assert_eq!(
            escape_plain_carve_syntax(input, HandledDelimiters::DJOT),
            input,
            "input {input:?}"
        );
    }
}

/// BOUND: an at-sign the source already escaped is not escaped twice.
#[test]
fn an_already_escaped_at_sign_is_left_alone() {
    assert_eq!(
        escape_plain_carve_syntax("hi \\@user ok", HandledDelimiters::DJOT),
        "hi \\@user ok"
    );
}

/// ONLY THE OPENING COLON. Escaping the opener is what makes the whole
/// shortcode text - the closing colon then has a letter against it and
/// opens nothing - so a second escape would be bytes PART 11 §4 asks the
/// writer not to spend. The corpus case `a-symbol-shortcode` pins the
/// one-escape form; this names WHICH colon under every profile.
#[test]
fn a_symbol_shortcode_is_frozen_at_its_opener_only() {
    for (input, expected) in [
        ("a :rocket: b", "a \\:rocket: b"),
        (":rocket:", "\\:rocket:"),
        // The reaction shortcodes: `+` and `-` open a name, `_` does not.
        ("a :+1: b", "a \\:+1: b"),
        ("a :-1: b", "a \\:-1: b"),
        // Two shortcodes against each other are two openers.
        (":a::b:", "\\:a:\\:b:"),
    ] {
        for (profile, handled) in profiles() {
            assert_eq!(
                escape_plain_carve_syntax(input, handled),
                expected,
                "{input:?} under {profile}"
            );
        }
    }
}

/// BOUND: THE NEAR NEIGHBOUR THAT MUST NOT MOVE. `a : b : c` is the shape a
/// rule over every colon would break, and the corpus pins it unchanged
/// (`a-colon-that-closes-no-shortcode`). The parser opens no symbol at
/// either colon - a space is not a name character - so neither is offered
/// an escape here.
#[test]
fn a_colon_that_opens_no_shortcode_is_left_bare() {
    for input in [
        // The corpus's own bound.
        "a : b : c",
        // A colon with a letter against it is not an opener.
        "https://example.com",
        "note: see below",
        "12:30:45",
        // A name that never closes.
        "a :rocket b",
        // The fence and the definition marker, which are line-initial
        // constructs the converters already own.
        ":::",
        ": definition",
        // Nothing after the colon at all.
        "ends with :",
    ] {
        for (profile, handled) in profiles() {
            assert_eq!(
                escape_plain_carve_syntax(input, handled),
                input,
                "{input:?} under {profile}"
            );
        }
    }
}

/// BOUND: `_` CANNOT OPEN A NAME, because `:_x_:` would steal from
/// underline - `parse_symbol` excludes it from the first name character's
/// class and this mirrors that. Asserted on the colon alone: the `_x_` in
/// it is a bare underline run, which the profiles that do not spell `_`
/// freeze for their own unrelated reason.
#[test]
fn an_underscore_does_not_open_a_symbol_name() {
    for (profile, handled) in profiles() {
        let got = escape_plain_carve_syntax("a :_x_: b", handled);
        assert!(!got.contains("\\:"), "{profile} escaped a colon in {got:?}");
    }
}

/// BOUND: a colon the source already escaped is not escaped twice.
#[test]
fn an_already_escaped_symbol_opener_is_left_alone() {
    assert_eq!(
        escape_plain_carve_syntax("a \\:rocket: b", HandledDelimiters::DJOT),
        "a \\:rocket: b"
    );
}

//! carve-rs' first lint rules: the two diagnostics PART 9 §10 implies and had
//! nowhere to say (markup-carve/carve#1131, markup-carve/carve#1132).
//!
//! Neither reports an engine defect. carve-rs, carve-js and carve-php render
//! every case below byte-identically and exactly as the clause reads; the rules
//! report the two places where the clause's own scope loses something an author
//! wrote, with nothing else marking it.
//!
//! Three things this file is built to keep honest, because a rule that cannot
//! fire is the defect these rules exist to remove:
//!
//! 1. Every off-span target the rule can reach is enumerated and asserted, not
//!    the six the ticket happened to list.
//! 2. The `cite`-on-block-quote carve-out is proved LIVE - the control asserts
//!    no diagnostic, and `a_reserved_name_on_a_quote_is_reported_when_it_is_not_valid_html`
//!    proves the same walk does reach a quote, so the control cannot pass by
//!    the rule never running.
//! 3. The node-type strings the messages name are pinned against `to_json`
//!    rather than trusted, so the walker cannot drift from the published wire
//!    vocabulary.

use carve::extensions::semantic_span::SemanticSpan;
use carve::{lint_carve, lint_carve_with_options, CarveExtension, LintWarning, Options};

const VALUE_IGNORED: &str = "semantic-attribute-value-ignored";
const OUTSIDE_SPAN: &str = "semantic-attribute-outside-span";

fn rules(src: &str) -> Vec<&'static str> {
    lint_carve(src).into_iter().map(|w| w.rule).collect()
}

fn with_semantic_span(src: &str) -> Vec<LintWarning> {
    let ext = SemanticSpan;
    let extensions: Vec<&dyn CarveExtension> = vec![&ext];
    let options = Options {
        extensions,
        ..Options::default()
    };
    lint_carve_with_options(src, &options)
}

fn rules_with_semantic_span(src: &str) -> Vec<&'static str> {
    with_semantic_span(src)
        .into_iter()
        .map(|w| w.rule)
        .collect()
}

// ---- markup-carve/carve#1131: the value only selects the wrapper ----

/// Core reserves three names. `abbr` and `time` carry their value to the
/// output (`title` / `datetime`); `kbd` does not, so it is the ONLY core name
/// this rule can fire on.
#[test]
fn a_value_on_a_core_name_that_drops_it_is_reported() {
    assert_eq!(rules("[x]{kbd=\"V\"}\n"), vec![VALUE_IGNORED]);
}

#[test]
fn the_core_names_that_carry_a_value_are_not_reported() {
    assert!(rules("[x]{abbr=\"V\"}\n").is_empty());
    assert!(rules("[x]{time=\"V\"}\n").is_empty());
}

/// A bare name loses nothing, so there is nothing to report.
#[test]
fn a_bare_name_is_not_reported() {
    assert!(rules("[x]{kbd}\n").is_empty());
    assert!(rules_with_semantic_span("[x]{samp}\n").is_empty());
}

/// The tier test, and the reason the rule takes the caller's extensions.
/// Without `SemanticSpan`, `samp` stays an ordinary attribute and its value
/// reaches the output intact - reporting it as discarded would report a loss
/// that is not happening.
#[test]
fn an_extension_name_is_reported_only_when_the_extension_is_registered() {
    assert!(rules("[x]{samp=\"V\"}\n").is_empty());
    assert_eq!(
        rules_with_semantic_span("[x]{samp=\"V\"}\n"),
        vec![VALUE_IGNORED]
    );
}

#[test]
fn every_extension_name_that_drops_its_value_is_reported() {
    for name in ["samp", "var", "cite"] {
        let src = format!("[x]{{{name}=\"V\"}}\n");
        assert_eq!(
            rules_with_semantic_span(&src),
            vec![VALUE_IGNORED],
            "{name} drops its value under SemanticSpan"
        );
    }
    // `dfn` maps its value to `title` exactly as `abbr` does, so it keeps it.
    assert!(rules_with_semantic_span("[x]{dfn=\"V\"}\n").is_empty());
}

#[test]
fn the_message_names_the_attribute_and_the_element() {
    let warnings = lint_carve("[x]{kbd=\"V\"}\n");
    assert_eq!(warnings.len(), 1);
    let message = &warnings[0].message;
    assert!(message.contains("\"kbd\""), "{message}");
    assert!(message.contains("<kbd>"), "{message}");
    assert!(message.contains("abbr, dfn and time"), "{message}");
}

// ---- markup-carve/carve#1132: the name is not on a span ----

/// The ticket lists six off-span targets. Every row here was MEASURED against
/// `carve --json` on this tree: the named type is the node the authored `kbd`
/// actually lands on. Twenty-eight distinct wire types can carry it, against
/// the six the ticket named, so the count is a floor rather than a total.
const OFF_SPAN_TARGETS: &[(&str, &str)] = &[
    // inline
    ("code", "`c`{kbd}\n"),
    ("link", "[t](http://e.com){kbd}\n"),
    ("image", "a ![a](i.png){kbd}\n"),
    ("emphasis", "/e/{kbd}\n"),
    ("strong", "*e*{kbd}\n"),
    ("strike", "~s~{kbd}\n"),
    ("underline", "_u_{kbd}\n"),
    ("superscript", "{^s^}{kbd}\n"),
    ("subscript", "{,s,}{kbd}\n"),
    ("highlight", "=h={kbd}\n"),
    ("insert", "{++i++}{kbd}\n"),
    ("delete", "{--d--}{kbd}\n"),
    ("autolink", "<http://e.com>{kbd}\n"),
    ("math", "$`x`{kbd}\n"),
    ("literal_inline", "!`c`{kbd}\n"),
    ("symbol", ":smile:{kbd}\n"),
    ("inline_extension", ":term[x]{kbd}\n"),
    ("footnote_ref", "x[^a]{kbd}\n\n[^a]: n\n"),
    ("inline_footnote", "x^[n]{kbd}\n"),
    // block
    ("paragraph", "{kbd}\nPara\n"),
    ("heading", "{kbd}\n# H\n"),
    ("block_quote", "{kbd}\n> q\n"),
    ("list", "{kbd}\n- a\n"),
    ("table", "{kbd}\n| a | b |\n|---|---|\n| 1 | 2 |\n"),
    ("table_row", "| a | b |{kbd}\n|---|---|\n| 1 | 2 |\n"),
    ("code_block", "{kbd}\n```\nc\n```\n"),
    ("thematic_break", "{kbd}\n---\n"),
    ("admonition", "{kbd}\n::: note\nx\n:::\n"),
];

#[test]
fn every_off_span_target_is_reported_and_named_by_its_wire_type() {
    for (node_type, src) in OFF_SPAN_TARGETS {
        let warnings = lint_carve(src);
        assert_eq!(
            warnings.iter().map(|w| w.rule).collect::<Vec<_>>(),
            vec![OUTSIDE_SPAN],
            "{node_type}: {src:?}"
        );
        assert!(
            warnings[0]
                .message
                .contains(&format!("on {node_type} it stays")),
            "{node_type}: message was {:?}",
            warnings[0].message
        );
    }
}

/// The walker names node types by hand; `to_json` publishes them. A mismatch
/// would put a type in a diagnostic that no other tool in the ecosystem uses,
/// so the two are pinned against each other rather than assumed equal.
#[test]
fn the_node_type_the_message_names_is_the_published_wire_type() {
    for (node_type, src) in OFF_SPAN_TARGETS {
        let json = carve::to_json(&carve::parse(src));
        assert!(
            json.contains(&format!("\"type\":\"{node_type}\"")),
            "{node_type} is not a wire type in {json}"
        );
    }
}

/// The whole point of the rule: one spelling means two things, and only the
/// span form gets the element.
#[test]
fn the_same_name_on_a_span_is_not_reported_as_outside_one() {
    assert!(rules("[x]{kbd}\n").is_empty());
}

/// Tier test for the other rule. A name core leaves an ordinary attribute is an
/// ordinary attribute everywhere, so `` `c`{samp} `` is not "outside the span"
/// until the extension makes `samp` mean something.
#[test]
fn an_extension_name_off_span_is_reported_only_when_the_extension_is_registered() {
    assert!(rules("`c`{samp}\n").is_empty());
    assert_eq!(rules_with_semantic_span("`c`{samp}\n"), vec![OUTSIDE_SPAN]);
}

/// A retired name (`code`, `mark`) is an ordinary attribute under every
/// configuration, so neither rule fires on it. Pinned so a future re-reservation
/// has to change a test rather than silently widening the rules.
#[test]
fn a_retired_name_is_reported_by_neither_rule() {
    for name in ["code", "mark"] {
        for src in [format!("[x]{{{name}=\"V\"}}\n"), format!("`c`{{{name}}}\n")] {
            assert!(rules(&src).is_empty(), "{src:?}");
            assert!(rules_with_semantic_span(&src).is_empty(), "{src:?}");
        }
    }
}

// ---- the carve-out: `cite` on a block quote ----

/// `cite` IS a URL attribute of `blockquote` and `q` in HTML, so
/// `{cite="…"}` on a quote is the author getting what they asked for. carve-js
/// carries this exception (markup-carve/carve-js#1022) and a port without it
/// diverges on the first quote carrying a citation URL.
#[test]
fn cite_on_a_block_quote_is_valid_html_and_is_not_reported() {
    let src = "{cite=\"https://example.org/dune\"}\n> q\n";

    // ANCHOR FIRST. Under a CORE render `cite` is not an element name at all,
    // so the rule never reaches the quote and an empty result says nothing
    // about the exception - a control in that state passes for the wrong
    // reason. Registering SemanticSpan makes `cite` live, which the code-span
    // case proves in this same render before the quote is asserted on.
    assert_eq!(
        rules_with_semantic_span("`c`{cite=\"https://example.org/dune\"}\n"),
        vec![OUTSIDE_SPAN],
        "cite must be a live element name here, or the assertion below is vacuous"
    );
    assert!(
        rules_with_semantic_span(src).is_empty(),
        "{:?}",
        with_semantic_span(src)
    );

    // Core reports nothing either, but only because `cite` is an ordinary
    // attribute there. Pinned as behavior, not as evidence for the exception.
    assert!(rules(src).is_empty());
}

/// The control above is only worth something if the walk reaches a block quote
/// at all. It does: the same node reports another reserved name, so an empty
/// result for `cite` is the exception firing rather than the rule being absent.
#[test]
fn a_reserved_name_on_a_quote_is_reported_when_it_is_not_valid_html() {
    let warnings = lint_carve("{kbd}\n> q\n");
    assert_eq!(
        warnings.iter().map(|w| w.rule).collect::<Vec<_>>(),
        vec![OUTSIDE_SPAN]
    );
    assert!(
        warnings[0].message.contains("on block_quote it stays"),
        "{:?}",
        warnings[0]
    );
}

/// The exception is scoped to the quote. `cite` anywhere else is still a raw
/// attribute with no meaning, so it is still reported.
#[test]
fn cite_off_a_block_quote_is_still_reported() {
    assert_eq!(
        rules_with_semantic_span("`c`{cite=\"u\"}\n"),
        vec![OUTSIDE_SPAN]
    );
    assert_eq!(
        rules_with_semantic_span("{cite=\"u\"}\nPara\n"),
        vec![OUTSIDE_SPAN]
    );
}

// ---- locations, containers and the clean case ----

#[test]
fn a_warning_locates_the_node_in_the_source() {
    let src = "lead\n\n`c`{kbd}\n";
    let warnings = lint_carve(src);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3);
    assert_eq!(warnings[0].column, 1);
    assert_eq!(&src[warnings[0].start..warnings[0].end], "`c`{kbd}");
}

/// Offsets are BYTES, so a caller can slice the source it passed. carve-js
/// reports UTF-16 for the same reason on its side; the unit follows the host
/// language rather than the other engine.
#[test]
fn offsets_are_byte_offsets_into_the_source() {
    let src = "é é `c`{kbd}\n";
    let warnings = lint_carve(src);
    assert_eq!(warnings.len(), 1);
    assert_eq!(&src[warnings[0].start..warnings[0].end], "`c`{kbd}");
}

/// A footnote definition hoists to the document (PART 9 §7), so its body is not
/// reachable from `document.children` and a walk of those alone is silent
/// inside every footnote.
#[test]
fn a_rule_fires_inside_a_hoisted_footnote_definition() {
    assert_eq!(rules("x[^a]\n\n[^a]: see `c`{kbd}\n"), vec![OUTSIDE_SPAN]);
}

#[test]
fn a_rule_fires_inside_a_container() {
    assert_eq!(rules("> `c`{kbd}\n"), vec![OUTSIDE_SPAN]);
    assert_eq!(rules("- `c`{kbd}\n"), vec![OUTSIDE_SPAN]);
    assert_eq!(
        rules("| `c`{kbd} | b |\n|---|---|\n| 1 | 2 |\n"),
        vec![OUTSIDE_SPAN]
    );
}

#[test]
fn both_rules_can_fire_on_one_document() {
    let mut found = rules("[x]{kbd=\"V\"}\n\n`c`{kbd}\n");
    found.sort_unstable();
    assert_eq!(found, vec![OUTSIDE_SPAN, VALUE_IGNORED]);
}

/// Every rule here reports a silent degradation in the document, so ordinary
/// Carve emits nothing. A linter that warns on clean input is one authors turn
/// off.
#[test]
fn ordinary_carve_produces_no_warnings() {
    let src = "# Title\n\nSome /text/ with a [link](http://e.com), `code`, an ![img](i.png)\n\
               and a [span]{.cls #id data-x=\"1\"}.\n\n\
               > quoted\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
    assert!(lint_carve(src).is_empty(), "{:?}", lint_carve(src));
    assert!(with_semantic_span(src).is_empty());
}

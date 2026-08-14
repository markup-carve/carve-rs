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
/// actually lands on. TWENTY-NINE distinct wire types can carry it, against the
/// six the ticket named, so the ticket's count is a floor rather than a total.
///
/// Three shapes are deliberately absent because carve-rs cannot reach them from
/// source, and their absence is measured too. A table CELL takes no attributes
/// (`| a{kbd} |` and `| {kbd}a |` both leave the braces literal) - the ROW does,
/// on the closing pipe. A `list_item` carries `attrs` in the AST but no source
/// spelling fills it; the block-attribute line binds to the `list`. And
/// `RawBlock` has no `attrs` field at all, so it is not a target here even
/// though it is one in carve-php (markup-carve/carve-php#1254).
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
    ("div", "{kbd}\n:::\nx\n:::\n"),
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

// ---- the tail quotes what the renderer emits (markup-carve/carve-js#1058) ----

/// The `kbd="…"` the HTML renderer actually writes for `src`, read out of the
/// document rather than restated.
///
/// The point of the rule's tail is that it agrees with the output, so the
/// expectation is taken FROM the output. A test that spelled the expected value
/// by hand would pass just as happily if the renderer changed underneath it.
fn emitted_kbd_attribute(src: &str) -> String {
    let html = carve::render_html(&carve::parse(src)).expect("render");
    let start = html.find("kbd=\"").expect("no kbd attribute in {html}") + "kbd=\"".len();
    let end = start + html[start..].find('"').expect("unterminated kbd attribute");
    html[start..end].to_string()
}

fn only_message(src: &str) -> String {
    let warnings = lint_carve(src);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    warnings[0].message.clone()
}

fn expected_message(node_type: &str, name: &str, emitted: &str) -> String {
    format!(
        "\"{name}\" is a semantic span attribute (PART 9 \u{a7}10) and only applies to an ordinary \
         [content]{{attrs}} span; on {node_type} it stays a raw attribute and renders as \
         {name}=\"{emitted}\"."
    )
}

/// The boolean form is the case the fixed `name=""` was right about, so it must
/// read exactly as it did before.
#[test]
fn the_boolean_form_still_reports_an_empty_value() {
    assert_eq!(emitted_kbd_attribute("`c`{kbd}\n"), "");
    assert_eq!(
        only_message("`c`{kbd}\n"),
        expected_message("code", "kbd", "")
    );
}

/// The case the fixed form got wrong: `` `c`{kbd="keyboard"} `` renders
/// `<code kbd="keyboard">`, and the message said `kbd=""`.
#[test]
fn an_authored_value_is_reported_as_the_renderer_emits_it() {
    let src = "`c`{kbd=\"keyboard\"}\n";
    assert_eq!(emitted_kbd_attribute(src), "keyboard");
    assert_eq!(
        only_message(src),
        expected_message("code", "kbd", "keyboard")
    );
}

/// The value is author text, so the message must not carry it raw. It is
/// escaped exactly as the renderer escapes an attribute value, which is what
/// makes the two agree character for character.
#[test]
fn the_quoted_value_is_escaped_the_way_the_renderer_escapes_it() {
    let src = "`c`{kbd=\"a<b>&'\\\"q\\\"\"}\n";
    let emitted = emitted_kbd_attribute(src);
    assert_eq!(emitted, "a&lt;b&gt;&amp;&apos;&quot;q&quot;");
    assert_eq!(only_message(src), expected_message("code", "kbd", &emitted));
}

/// A long value is elided rather than printed whole: a diagnostic is read on
/// one line, and an attribute carrying a paragraph would push the explanation
/// off it. 64 characters are kept.
#[test]
fn a_long_value_is_truncated_at_sixty_four_characters() {
    let long = "x".repeat(200);
    let src = format!("`c`{{kbd=\"{long}\"}}\n");
    assert_eq!(emitted_kbd_attribute(&src), long);
    assert_eq!(
        only_message(&src),
        expected_message("code", "kbd", &format!("{}\u{2026}", "x".repeat(64)))
    );
}

/// The boundary, both sides of it. Exactly 64 characters is printed whole, so
/// the cap is a cap and not an off-by-one that elides ordinary values.
#[test]
fn a_value_of_exactly_the_cap_is_not_truncated() {
    let src = format!("`c`{{kbd=\"{}\"}}\n", "x".repeat(64));
    assert_eq!(
        only_message(&src),
        expected_message("code", "kbd", &"x".repeat(64))
    );

    let src = format!("`c`{{kbd=\"{}\"}}\n", "x".repeat(65));
    assert_eq!(
        only_message(&src),
        expected_message("code", "kbd", &format!("{}\u{2026}", "x".repeat(64)))
    );
}

/// The cut is made on the SOURCE value and escaped afterwards. Cutting the
/// escaped text instead would split an entity and print a fragment such as
/// `&qu` as though the author had written it.
#[test]
fn truncation_never_splits_an_escaped_entity() {
    let value = format!("{}<tail", "y".repeat(63));
    let src = format!("`c`{{kbd=\"{value}\"}}\n");
    let message = only_message(&src);
    assert_eq!(
        message,
        expected_message("code", "kbd", &format!("{}&lt;\u{2026}", "y".repeat(63)))
    );
    assert!(!message.contains("&l\u{2026}"), "{message}");
}

/// The sibling rule was checked for the same assumption and carries none: it
/// names the attribute and says the value reaches no output, which is true of
/// every value. Pinned so a later edit cannot quietly give it one.
#[test]
fn the_value_ignored_message_quotes_no_value() {
    let warnings = lint_carve("[x]{kbd=\"keyboard\"}\n");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].rule, VALUE_IGNORED);
    assert_eq!(
        warnings[0].message,
        "Value on the semantic attribute \"kbd\" is discarded: it selects the <kbd> element and \
         reaches no output. Only abbr, dfn and time carry a value (as title or datetime)."
    );
    assert!(!warnings[0].message.contains("keyboard"), "{warnings:?}");
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

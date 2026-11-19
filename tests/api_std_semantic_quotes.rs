//! PROVISIONAL `semanticSpan` and `smartQuotes` of `carve::api::extensions`,
//! ported from carve-js (test/extensions/blocks-inline.test.ts).

use carve::api::extensions::{self, Bundle, SmartQuotesOptions};
use carve::api::{self, Carve, LintOptions, Options};

fn html_with(source: &str, bundle: Bundle, opts: &Options) -> String {
    let mut carve = Carve::new();
    carve.register(bundle).unwrap();
    carve.render(source, opts).unwrap()
}

fn smart() -> Options {
    Options {
        smart_typography: true,
        ..Options::default()
    }
}

fn quotes(source: &str, options: &SmartQuotesOptions) -> String {
    html_with(source, extensions::smart_quotes(options), &smart())
}

fn locale(name: &str) -> SmartQuotesOptions {
    SmartQuotesOptions {
        locale: Some(name.into()),
        ..SmartQuotesOptions::default()
    }
}

fn lint_codes(source: &str, extensions: &[&str]) -> Vec<String> {
    let opts = LintOptions {
        extensions: extensions.iter().map(|e| (*e).to_string()).collect(),
        ..LintOptions::default()
    };
    api::lint(source, &opts)
        .unwrap()
        .into_iter()
        .map(|e| e.code)
        .collect()
}

#[test]
fn semantic_span_renders_the_four_added_keys_in_the_0_1_nesting_order() {
    let html = |s: &str| html_with(s, extensions::semantic_span(), &Options::default());
    assert_eq!(
        html("[x]{samp} [y]{var} [z]{cite} [w]{dfn=term}"),
        "<p><samp>x</samp> <var>y</var> <cite>z</cite> <dfn title=\"term\">w</dfn></p>"
    );
    assert_eq!(html("[x]{kbd samp}"), "<p><kbd><samp>x</samp></kbd></p>");
    assert_eq!(
        api::render("[x]{samp}", &Options::default()).unwrap(),
        "<p><span samp=\"\">x</span></p>"
    );
}

#[test]
fn semantic_span_nests_with_the_core_keys_and_keeps_an_authored_title_outside() {
    let html = |s: &str| html_with(s, extensions::semantic_span(), &Options::default());
    assert_eq!(
        html("[x]{abbr=A dfn=D title=T}"),
        "<p><dfn title=\"T\"><abbr title=\"A\">x</abbr></dfn></p>"
    );
    assert_eq!(html("[x]{Samp=1 cite}"), "<p><cite Samp=\"1\">x</cite></p>");
}

#[test]
fn semantic_span_makes_lint_report_the_added_keys_outside_a_span() {
    let source = "{samp}\nOutput text.\n";
    assert_eq!(
        lint_codes(source, &["carve.semanticSpan"]),
        ["semantic-attribute-outside-span"]
    );
    assert_eq!(lint_codes(source, &[]), Vec::<String>::new());
    assert_eq!(
        lint_codes("[x]{samp=1}\n", &["carve.semanticSpan"]),
        ["semantic-attribute-value-ignored"]
    );
}

#[test]
fn smart_quotes_write_the_locale_quotes_and_leave_apostrophes() {
    assert_eq!(
        quotes("\"Hi\" it's 'x'", &locale("de")),
        "<p>„Hi“ it’s ‚x‘</p>"
    );
    let off = Options {
        smart_typography: false,
        ..Options::default()
    };
    let plain = html_with("\"Hi\"", extensions::smart_quotes(&locale("de")), &off);
    assert_eq!(plain, "<p>\"Hi\"</p>");
}

#[test]
fn smart_quotes_use_the_exact_locale_then_its_language_then_english() {
    assert_eq!(quotes("\"a\"", &locale("de_CH")), "<p>«a»</p>");
    assert_eq!(quotes("\"a\"", &locale("fr-CA")), "<p>«&nbsp;a&nbsp;»</p>");
    assert_eq!(quotes("\"a\"", &locale("xx")), "<p>“a”</p>");
    let close = SmartQuotesOptions {
        close_double_quote: Some("!".into()),
        ..SmartQuotesOptions::default()
    };
    assert_eq!(quotes("\"a\"", &close), "<p>“a!</p>");
}

#[test]
fn smart_quotes_list_the_supported_locales() {
    let locales = extensions::smart_quote_locales();
    assert_eq!(locales.len(), extensions::SMART_QUOTE_LOCALES.len());
    assert_eq!(locales[0], "en");
    assert!(extensions::is_smart_quote_locale_supported("de-AT"));
    assert!(!extensions::is_smart_quote_locale_supported("xx"));
}

#[test]
fn both_select_by_name_on_a_host() {
    let mut carve = Carve::new();
    carve.enable_standard("carve.semanticSpan", true).unwrap();
    carve.enable_standard("carve.smartQuotes", true).unwrap();
    assert_eq!(
        carve.render("[x]{var} \"y\"", &smart()).unwrap(),
        "<p><var>x</var> “y”</p>"
    );
}

#[test]
fn outside_span_reads_the_keys_of_the_node_not_those_of_its_inline_spans() {
    assert_eq!(
        lint_codes(
            "{kbd=V}\nPara [[x]{abbr}]{samp}.\n",
            &["carve.semanticSpan"]
        ),
        ["semantic-attribute-outside-span"]
    );
    assert_eq!(
        lint_codes("{abbr=V}\nPara [x]{abbr=V}.\n", &[]),
        ["semantic-attribute-outside-span"]
    );
}

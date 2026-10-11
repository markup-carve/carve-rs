use carve::{CarveExtension, Options};

struct GeneralParser;
impl CarveExtension for GeneralParser {
    fn name(&self) -> &'static str {
        "word-fallback"
    }
}

#[test]
fn short_words_match_general_parser_with_positions_and_abbreviations() {
    let fallback = GeneralParser;
    for word in [
        "x".to_string(),
        "HTML".to_string(),
        "a".repeat(64),
        "a".repeat(65),
        "é".to_string(),
        "a b".to_string(),
    ] {
        for marker in ['*', '/', '_', '-', '+', '^', ',', '~', '='] {
            for source in [
                format!("{marker}{word}{marker}"),
                format!("{{{marker}{word}{marker}}}{{k=\"*}}\"}}"),
                format!("😀 prefix {{{marker}{word}{marker}}} suffix"),
                format!("> - {{{marker}{word}{marker}}}\n"),
                format!("| {{{marker}{word}{marker}}}\n|   lazy {{{marker}{word}{marker}}}\n"),
                format!("|\t{{{marker}{word}{marker}}}\n"),
                format!("*[HTML]: HyperText Markup Language\n\n{{{marker}{word}{marker}}}"),
            ] {
                for positions in [false, true] {
                    let direct = Options::new().with_positions(positions);
                    let general = Options::new()
                        .with_positions(positions)
                        .with_extension(&fallback);
                    assert_eq!(
                        carve::parse_with_options(&source, &direct),
                        carve::parse_with_options(&source, &general),
                        "source: {source:?}, positions: {positions}",
                    );
                }
            }
        }
    }
}

struct WordMatcher;
impl CarveExtension for WordMatcher {
    fn name(&self) -> &'static str {
        "word-matcher"
    }
    fn match_inline(
        &self,
        text: &str,
        pos: usize,
        _: &carve::MatcherContext<'_>,
    ) -> Option<carve::InlineMatch> {
        text.get(pos..)?
            .starts_with("alpha")
            .then(|| carve::InlineMatch {
                node: carve::InlineNode::text("matched".to_string()),
                end: pos + 5,
            })
    }
}

#[test]
fn extensions_can_claim_words_inside_emphasis() {
    let extension = WordMatcher;
    assert_eq!(
        carve::to_html_with_options(
            "*alpha* {*alpha*}",
            &Options::new().with_extension(&extension)
        ),
        "<p><strong>matched</strong> <strong>matched</strong></p>",
    );
}

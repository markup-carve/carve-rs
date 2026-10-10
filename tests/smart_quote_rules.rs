use carve::{Options, SmartQuotes};

#[test]
fn javascript_smart_quote_rule_cases() {
    for (source, expected) in [
        (
            r###"[x]{.c}"q""###,
            r###"<p><span class="c">x</span>”q”</p>"###,
        ),
        (
            r###"[x]{.c}{%%}"q""###,
            r###"<p><span class="c">x</span>”q”</p>"###,
        ),
        (r###"word-"{%%}q""###, r###"<p>word-“q”</p>"###),
        (
            r###"'one {%%}'two' end'"###,
            r###"<p>‘one ’two’ end’</p>"###,
        ),
        (
            r###""Interrupted---" he said."###,
            r###"<p>“Interrupted—” he said.</p>"###,
        ),
        (
            r###"'Interrupted---' he said."###,
            r###"<p>‘Interrupted—’ he said.</p>"###,
        ),
        (r###"'tisn't 'twasn't"###, r###"<p>’tisn’t ’twasn’t</p>"###),
        (r###"'em'2"###, r###"<p>’em’2</p>"###),
        (r###"'em'é"###, r###"<p>’em’é</p>"###),
        (r###"'tissue"###, r###"<p>‘tissue</p>"###),
        (r###"'tisé"###, r###"<p>‘tisé</p>"###),
        (r###"'one 'two' end'"###, r###"<p>‘one ’two’ end’</p>"###),
        (
            r###"'one don't 'two' end'"###,
            r###"<p>‘one don’t ’two’ end’</p>"###,
        ),
        (
            r###"'one '70s 'two' end'"###,
            r###"<p>‘one ’70s ’two’ end’</p>"###,
        ),
        (
            r###"'one é'é 'two' end'"###,
            r###"<p>‘one é’é ’two’ end’</p>"###,
        ),
        (
            r###"'one 'tis 'two' end'"###,
            r###"<p>‘one ’tis ’two’ end’</p>"###,
        ),
        (r###"say 'word"###, r###"<p>say ’word</p>"###),
        (r###"'word"###, r###"<p>‘word</p>"###),
        (r###"say "'word"###, r###"<p>say “‘word</p>"###),
        (
            r###"say '*bold* text"###,
            r###"<p>say ‘<strong>bold</strong> text</p>"###,
        ),
        (r###"say ' word"###, r###"<p>say ‘ word</p>"###),
        (
            r###"'*bold* 'inner'"###,
            r###"<p>‘<strong>bold</strong> ’inner’</p>"###,
        ),
        (r###"say 'one' 'two"###, r###"<p>say ‘one’ ’two</p>"###),
        (r###"say 'word ' end"###, r###"<p>say ’word ‘ end</p>"###),
        (
            r###"'one *'two* end'"###,
            r###"<p>‘one <strong>’two</strong> end’</p>"###,
        ),
        (
            r###"say *x 'word*"###,
            r###"<p>say <strong>x ’word</strong></p>"###,
        ),
        (
            r###"'one [x 'two](url) end'"###,
            r###"<p>‘one <a href="url">x ’two</a> end’</p>"###,
        ),
        (
            r###"'one

'next'"###,
            r###"<p>‘one</p>
<p>‘next’</p>"###,
        ),
        (r###"\'tis \"word"###, r###"<p>'tis "word</p>"###),
        (
            r###"`'tis` and `'word`{=html}"###,
            r###"<p><code>'tis</code> and 'word</p>"###,
        ),
        (
            r###"[x](https://example.com/'tis)"###,
            r###"<p><a href="https://example.com/&apos;tis">x</a></p>"###,
        ),
        (
            r###"[x]{title="'tis"}"###,
            r###"<p><span title="&apos;tis">x</span></p>"###,
        ),
    ] {
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}

#[test]
fn dash_contexts_and_elision_words() {
    for dash in ['-', '–', '—'] {
        for next in [
            "", " ", "\t", "\n", "\u{a0}", "\"", "'", ".", ",", ";", ":", "!", "?", ")", "]",
        ] {
            for (quote, closing) in [('"', '”'), ('\'', '’')] {
                let source = format!("word{dash}{quote}{next}");
                assert!(
                    carve::to_html(&source).contains(&format!("word{dash}{closing}")),
                    "{source:?}"
                );
            }
        }
        for (quote, opening, closing) in [('"', '“', '”'), ('\'', '‘', '’')] {
            assert_eq!(
                carve::to_html(&format!("word{dash}{quote}Hello{quote}")),
                format!("<p>word{dash}{opening}Hello{closing}</p>")
            );
        }
    }
    for word in [
        "tis", "tisn", "twas", "twasn", "twere", "twill", "twould", "em", "cause", "til", "n",
        "bout",
    ] {
        for spelling in [word.to_string(), word.to_uppercase()] {
            assert_eq!(
                carve::to_html(&format!("'{spelling} here")),
                format!("<p>’{spelling} here</p>")
            );
            assert_eq!(
                carve::to_html(&format!("'{spelling}'")),
                format!("<p>‘{spelling}’</p>")
            );
        }
    }
}

#[test]
fn block_state_resets() {
    for (source, expected) in [
        ("# 'one\n\n'next'", "<p>‘next’</p>"),
        ("- 'one\n- 'next'", "<li>‘next’</li>"),
        ("| 'one | 'next' |\n|---|---|", "‘next’</th>"),
    ] {
        assert!(carve::to_html(source).contains(expected), "{source:?}");
    }
}

#[test]
fn apostrophe_locales_and_source_round_trips() {
    let extension = SmartQuotes::new("de");
    let options = Options::new().with_extension(&extension);
    assert_eq!(
        carve::to_html_with_options("say 'word and 'tis", &options),
        "<p>say ’word and ’tis</p>"
    );
    assert_eq!(
        carve::to_html_with_options("'word'", &options),
        "<p>‚word‘</p>"
    );
    for source in ["say 'word", "'tis"] {
        assert_eq!(carve::to_carve(source), format!("{source}\n"));
        let json = carve::to_json_with_options(source, &Options::new());
        assert!(json.contains("right_single_quote"));
        assert!(!json.contains("left_single_quote"));
        let decoded = carve::from_json(&json).expect("decode quote nodes");
        assert_eq!(
            carve::render_html(&decoded).unwrap(),
            carve::to_html(source)
        );
        assert_eq!(
            carve::to_html(&carve::to_carve(source)),
            carve::to_html(source)
        );
    }
}

#[test]
fn invisible_comments_preserve_previous_character() {
    for block in ["{%%}", "{% hidden %}", "{%%}{% hidden %}"] {
        for (prefix, suffix, expected) in [
            ("\\{", "\"q\"", "{“q”"),
            ("(", "\"q\"", "(“q”"),
            ("", "\"q\"", "“q”"),
            ("x", "\"q\"", "x”q”"),
            ("say ", "'word", "say ’word"),
            ("\"", "'word", "“‘word"),
            ("\\{", "'q'", "{‘q’"),
        ] {
            let source = format!("{prefix}{block}{suffix}");
            assert_eq!(
                carve::to_html(&source),
                format!("<p>{expected}</p>"),
                "{source:?}"
            );
        }
        for dash in ["-", "--", "---", "–", "—"] {
            let glyph = match dash {
                "--" => "–",
                "---" => "—",
                _ => dash,
            };
            for (quote, opening, closing) in [('"', '“', '”'), ('\'', '‘', '’')] {
                assert_eq!(
                    carve::to_html(&format!("word{dash}{block}{quote} ")),
                    format!("<p>word{glyph}{closing}</p>")
                );
                assert_eq!(
                    carve::to_html(&format!("word{dash}{block}{quote}q{quote}")),
                    format!("<p>word{glyph}{opening}q{closing}</p>")
                );
            }
        }
    }
    assert_eq!(
        carve::to_html("[x]{.c}\"q\""),
        "<p><span class=\"c\">x</span>”q”</p>"
    );
}

#[test]
fn source_typography_still_emits_straight_quotes() {
    let options = Options {
        smart_typography: carve::SmartTypographyMode::Source,
        ..Options::new()
    };
    assert_eq!(
        carve::to_html_with_options("say 'word and 'tis", &options),
        "<p>say 'word and 'tis</p>"
    );
}

#[test]
fn unicode_letter_runs_and_nested_opener_handles() {
    for (source, expected) in [
        ("'tis\u{345}", "<p>’tis\u{345}</p>"),
        ("say 'word\u{345}", "<p>say ’word\u{345}</p>"),
        (
            "say *x /y 'word/*",
            "<p>say <strong>x <em>y ’word</em></strong></p>",
        ),
        (
            "say {~x 'word~>replacement~}",
            "<p>say <del>x ’word</del><ins>replacement</ins></p>",
        ),
    ] {
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}

#[test]
fn inline_footnotes_have_independent_quote_scopes() {
    for (source, expected) in [
        (
            "say ^['note] 'two'",
            vec!["</sup></a> ‘two’</p>", "<p>‘note<a href=\"#fnref1\""],
        ),
        (
            "say 'one ^[say 'note] 'two' end'",
            vec![
                "<p>say ‘one ",
                "</sup></a> ’two’ end’</p>",
                "<p>say ’note<a href=\"#fnref1\"",
            ],
        ),
        (
            "say 'one ^['note'] 'two' end'",
            vec!["</sup></a> ’two’ end’</p>", "<p>‘note’<a href=\"#fnref1\""],
        ),
        (
            "say 'one ^[note'] end",
            vec!["<p>say ’one ", "<p>note’<a href=\"#fnref1\""],
        ),
        (
            "say ^[say *x 'note*] ^['other] 'two'",
            vec![
                "<strong>x ’note</strong>",
                "<p>‘other<a href=\"#fnref2\"",
                "</sup></a> ‘two’</p>",
            ],
        ),
        (
            "say ^['one [x 'two](url) end'] 'three'",
            vec![
                "‘one <a href=\"url\">x ’two</a> end’",
                "</sup></a> ‘three’</p>",
            ],
        ),
    ] {
        let output = carve::to_html(source);
        for fragment in expected {
            assert!(output.contains(fragment), "{source:?}: {output}");
        }
    }
    let extension = SmartQuotes::new("de");
    let options = Options::new().with_extension(&extension);
    let source = "say ^[say 'note] 'two'";
    let output = carve::to_html_with_options(source, &options);
    assert!(output.contains("<p>say ’note<a href=\"#fnref1\""));
    assert!(output.contains("</sup></a> ‚two‘</p>"));
    assert!(carve::to_carve(source).contains("say 'note"));
    assert!(carve::to_json_with_options(source, &Options::new()).contains("right_single_quote"));
}

#[test]
fn quotes_read_emitted_context_after_comments() {
    for (source, expected) in [
        (r###"x {% hidden %}"q""###, "<p>x “q”</p>"),
        ("x {% hidden %}'q'", "<p>x ‘q’</p>"),
        (r###"a{%%}"q""###, "<p>a”q”</p>"),
        (r###"[x]{.c}"q""###, "<p><span class=\"c\">x</span>”q”</p>"),
        (r###"\{{%%}"q""###, "<p>{“q”</p>"),
        (r###"{% c %}"q""###, "<p>“q”</p>"),
        (r###"x---{% c %}" y"###, "<p>x—” y</p>"),
    ] {
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}

//! carve#6713: off a span, every spelling of `abbr`, `time` and `kbd` merges and the last value written wins (C2-A-035).

use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    Carve::new()
        .render(source, &Options::default())
        .unwrap()
        .trim_end()
        .to_owned()
}

#[test]
fn last_written_spelling_wins_off_a_span() {
    let cases = [
        ("{abbr=a ABBR=b}{abbr=c}\npara\n", r#"<p abbr="c">para</p>"#),
        ("{abbr=a ABBR=b abbr=c}\npara\n", r#"<p abbr="c">para</p>"#),
        (
            "{kbd=a}\n{KBD=b}\n{kbd=c}\npara\n",
            r#"<p kbd="c">para</p>"#,
        ),
        (
            "`c`{abbr=a ABBR=b abbr=c}\n",
            r#"<p><code abbr="c">c</code></p>"#,
        ),
        (
            "*x*{time=a TIME=b}{time=c}\n",
            r#"<p><strong time="c">x</strong></p>"#,
        ),
        (
            "[x](u){abbr=a ABBR=b abbr=c}\n",
            r#"<p><a href="u" abbr="c">x</a></p>"#,
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

#[test]
fn spellings_stay_apart_on_a_span() {
    assert_eq!(
        html("[x]{abbr=a ABBR=b}{abbr=c}\n"),
        r#"<p><abbr title="c" ABBR="b">x</abbr></p>"#
    );
}

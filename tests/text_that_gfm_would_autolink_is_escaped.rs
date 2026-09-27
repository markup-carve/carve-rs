//! PART 11 §8i: the `:` of `http://`, `https://` or `ftp://` and the `.` of
//! `www.` are escaped where a GFM reader would autolink the text, read on the
//! emitted line.

fn md(src: &str) -> String {
    carve::to_markdown(src)
}

#[test]
fn each_form_is_escaped_where_a_reader_links_it() {
    for (src, out) in [
        ("See https://x.io now\n", "See https\\://x.io now\n"),
        (
            "HTTP://A.B and ftp://c.d\n",
            "HTTP\\://A.B and ftp\\://c.d\n",
        ),
        ("1https://x.io\n", "1https\\://x.io\n"),
        ("(www.z.org) /www.y.org\n", "(www\\.z.org) /www\\.y.org\n"),
        ("*a* www.y.org\n", "**a** www\\.y.org\n"),
    ] {
        assert_eq!(md(src), out, "{src:?}");
    }
}

#[test]
fn a_letter_before_the_form_leaves_it_bare() {
    for src in [
        "ahttps://x.io\n",
        "1www.y.org\n",
        "awww.y.org\n",
        "https:/x.io\n",
        "www\n",
    ] {
        assert_eq!(md(src), src, "{src:?}");
    }
}

#[test]
fn a_form_split_across_spans_is_one_form() {
    assert_eq!(
        md("[http]{.a}[s://x.io]{.b} and [ww]{.a}[w.y.org]{.b}\n"),
        "https\\://x.io and www\\.y.org\n"
    );
}

#[test]
fn code_and_link_text_are_not_escaped() {
    assert_eq!(
        md("`https://x.io` and [https://x.io](https://x.io)\n"),
        "`https://x.io` and [https://x.io](https://x.io)\n"
    );
}

#[test]
fn an_ordered_marker_split_across_spans_is_still_escaped() {
    assert_eq!(
        md("[1]{.a}[. x]{.b}\n\n[1]{.a}[.]{.b}\n\nsee [1]{.a}[. x]{.b}\n"),
        "1\\. x\n\n1\\.\n\nsee 1. x\n"
    );
}

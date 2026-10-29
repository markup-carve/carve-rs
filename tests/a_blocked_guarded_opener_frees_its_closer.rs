//! A guarded opener forms only where its guard opens around the pair. When the
//! guard cannot, the opener is blocked and its closer and block go to the next
//! reading (carve-rs#3505, carve-rs#3510, carve-rs#3512, carve-rs#3515).

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src).trim().to_string()
}

#[test]
fn the_reported_shapes_match_the_spec_model() {
    let rows = [
        ("_*a*{t=\"_\"}\n", "<p><u><strong>a</strong>{t=“</u>”}</p>"),
        (
            "_*a*{t=\"_\"} /a/{t=a_}\n",
            "<p><u><strong>a</strong>{t=“</u>”} <em t=\"a_\">a</em></p>",
        ),
        ("{_~b~{c_}\n", "<p><u><s>b</s>{c</u></p>"),
        ("{c_}x{_/==~/{c_}\n", "<p>{c_}x<u><em>==~</em>{c</u></p>"),
        ("{_*/a/ b/{c_}\n", "<p><u>*<em>a</em> b/{c</u></p>"),
        ("_*//*{c_}\n", "<p><u><strong>//</strong>{c</u>}</p>"),
    ];
    for (src, want) in rows {
        assert_eq!(html(src), want, "{src:?}");
    }
}

/// The guard's only closer sits in the block, so the guarded `/` is blocked and
/// the later `/b/` takes the closer and the block.
#[test]
fn a_later_opener_takes_the_closer_of_a_blocked_guarded_pair() {
    let rows = [
        ("_/a /b/{t=a_}\n", "<p>_/a <em t=\"a_\">b</em></p>"),
        ("_*a *b*{t=a_}\n", "<p>_*a <strong t=\"a_\">b</strong></p>"),
        (
            "_/=a={t=\"_\"} /a/{t=a_}\n",
            "<p>_/<mark t=\"_\">a</mark> <em t=\"a_\">a</em></p>",
        ),
        (
            "> ~q~ _/~b c~{t=\"_\"} /a/{t=a_}\n",
            "<blockquote><p><s>q</s> _/<s t=\"_\">b c</s> <em t=\"a_\">a</em></p></blockquote>",
        ),
    ];
    for (src, want) in rows {
        assert_eq!(html(src), want, "{src:?}");
    }
}

/// `_=_` closes the guard inside the `=` pair, so that pair is blocked, its
/// block is text, and the `/` in it closes the italic.
#[test]
fn a_guard_closing_inside_the_pair_blocks_it() {
    let rows = [
        ("/_=_={t=\"/\"}\n", "<p><em><u>=</u>={t=“</em>”}</p>"),
        (
            "a b /_~_~{t=\"//\"} ~c~{.k}\n",
            "<p>a b <em><u>~</u>~{t=“</em>/“} <s class=\"k\">c</s></p>",
        ),
    ];
    for (src, want) in rows {
        assert_eq!(html(src), want, "{src:?}");
    }
}

/// A `{_` that does not form guards as a bare `_`: with a bare closer behind
/// the block, the guarded pair takes the block and its `_}`.
#[test]
fn a_braced_marker_with_a_bare_closer_behind_the_block_guards_the_pair() {
    let rows = [
        (
            "{_*~/x/~*{c_} a_ b\n",
            "<p>{<u><strong c_=\"\"><s><em>x</em></s></strong> a</u> b</p>",
        ),
        (
            "{_/*x*/{c_} c_ d/\n",
            "<p>{<u><strong c_=\"\"><em>x</em></strong> c</u> d/</p>",
        ),
        (
            "{_/=x= y/{c_} a_ b\n",
            "<p>{<u><em c_=\"\"><mark>x</mark> y</em> a</u> b</p>",
        ),
        (
            "`_x` {_*~/x/~*{c_} a_ b\n",
            "<p><code>_x</code> {<u><strong c_=\"\"><s><em>x</em></s></strong> a</u> b</p>",
        ),
        // An open bare `_` makes the marker a nested `_`, and a first `_`
        // inside the pair closes it there: the braced span forms in both.
        (
            "_z {_~a_b~{c_} a_ b\n",
            "<p><u>z <u><s>a_b</s>{c</u> a</u> b</p>",
        ),
        (
            "a {_*=_=*{c__} c_ d/\n",
            "<p>a <u><strong><mark>_</mark></strong>{c_</u> c_ d/</p>",
        ),
    ];
    for (src, want) in rows {
        assert_eq!(html(src), want, "{src:?}");
    }
}

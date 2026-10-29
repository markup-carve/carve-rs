//! PART 9 E2: a closer demotes the openers above it, so a pair whose opener
//! sits in a span closing first does not form, and its block is text (corpus
//! 776, carve-rs#3528). Expected HTML is the spec model's.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src).trim().to_string()
}

#[test]
fn a_pair_crossing_an_open_span_does_not_hide_its_block() {
    let rows = [
        ("_/*a//*{t=_} b_\n", "<p><u><em>*a</em>/*{t=</u>} b_</p>"),
        (
            "_/=//={t=_} b_\n",
            "<p><u><em>=</em>/<mark>{t</mark></u>} b_</p>",
        ),
        (
            "a b _/*a//*{t=\"__\"} b_\n",
            "<p>a b <u><em>*a</em>/*{t=“</u>_”} b_</p>",
        ),
        (
            "~*x _a b*_{t='~'} z~\n",
            "<p><s><strong>x _a b</strong>_{t=‘</s>’} z~</p>",
        ),
        (
            "{/=~a b=~{t=/} b/}\n",
            "<p><em><mark>~a b</mark>~{t=</em> b/}</p>",
        ),
        (
            "{_~x *b c~~*{t=_} b_} a_ b\n",
            "<p><u><s>x *b c</s>~*{t=</u> b_} a_ b</p>",
        ),
    ];
    for (src, want) in rows {
        assert_eq!(html(src), want, "{src:?}");
    }
}

/// The italic is guarded by the underline, which closes nowhere else, so
/// pairing blocks it and the strike keeps the block.
#[test]
fn a_blocked_crossing_opener_leaves_the_block_attached() {
    assert_eq!(
        html("_/~/b/~{t=_} b=\n"),
        "<p>_/<s t=\"_\"><em>b</em></s> b=</p>"
    );
}

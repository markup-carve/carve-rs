//! An attribute block hides a closer only when the pair that takes it forms in
//! the reader's scope; otherwise it is text and its closer counts
//! (carve-rs#3506, carve-rs#3509).

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src).trim().to_string()
}

/// A guard with no closer, of the span's own kind (E3), or closing past the
/// span (E2a) does not open, so the pair it guards is text.
#[test]
fn a_guard_that_cannot_open_leaves_the_block_text() {
    let rows = [
        (
            "{*x /_a_{t=a*} b*}\n",
            "<p><strong>x /<u t=\"a*\">a</u> b</strong></p>",
        ),
        (
            "{*x _/a/{t=a*} b*}\n",
            "<p><strong>x _<em t=\"a*\">a</em> b</strong></p>",
        ),
        (
            "{=x /_a_{t=a=} b=}\n",
            "<p><mark>x /<u t=\"a=\">a</u> b</mark></p>",
        ),
        ("{_x _*a*{t=a_} b_}\n", "<p><u>x _*a*{t=a</u> b_}</p>"),
        (
            "{/x _*a*{t=a/} [a_] /} e_\n",
            "<p><em>x _<strong t=\"a/\">a</strong> [a_] </em> e_</p>",
        ),
        (
            "{*x /_=a={t=\"*}\"} a_ b*}\n",
            "<p><strong>x /<u><mark t=\"*}\">a</mark> a</u> b</strong></p>",
        ),
    ];
    for (src, expected) in rows {
        assert_eq!(html(src), expected, "{src:?}");
    }
}

/// A guard that closes inside the span opens, and its pair keeps the block.
#[test]
fn a_guard_that_opens_keeps_the_block_attached() {
    assert_eq!(
        html("{*x /_a_{t=a*} b/ c*}\n"),
        "<p><strong>x <em><u t=\"a*\">a</u> b</em> c</strong></p>"
    );
    assert_eq!(
        html("{*x _a_{t=a*} b*}\n"),
        "<p><strong>x <u t=\"a*\">a</u> b</strong></p>"
    );
}

/// The bare walk reads a guard met mid-walk the same way, and a guard whose
/// only closer sits in its host's block closes there.
#[test]
fn the_bare_walk_reads_guards_mid_walk() {
    assert_eq!(
        html("=a _*a*{t=\"q\"} c_\n"),
        "<p>=a <u><strong t=\"q\">a</strong> c</u></p>"
    );
    assert_eq!(
        html("/a _~a~{t=a_} c/\n"),
        "<p><em>a _<s t=\"a_\">a</s> c</em></p>"
    );
    assert_eq!(
        html("~a /*a*{t=a/} c~\n"),
        "<p><s>a /<strong t=\"a/\">a</strong> c</s></p>"
    );
}

/// The closer of a combined token whose block is attached opens nothing, so a
/// later unattached block is text and its closer closes.
#[test]
fn a_combined_token_closer_opens_nothing() {
    assert_eq!(
        html("_x /*a*/{.q} b/{c_} c_\n"),
        "<p><u>x <strong class=\"q\"><em>a</em></strong> b/{c</u>} c_</p>"
    );
    assert_eq!(
        html("_x /*a*/{.q} b/{c_}\n"),
        "<p><u>x <strong class=\"q\"><em>a</em></strong> b/{c</u>}</p>"
    );
}

//! A strong around a single italic writes `***a***`, and the reader sees ONE
//! triple-star run whose inside character is the content, not the nested `*`.
//! The flanking test has to ask about that merged run, as it already did for an
//! italic around a strong. CARVE-P11-045 (PART 11 §8c) takes the commuted
//! `***` read-back as the same construct and requires the delimiter spelling
//! wherever it reads back.

fn readback(source: &str) -> String {
    let md = carve::to_markdown(source);
    carve::render_html(&carve::markdown_to_ast(&md)).expect("the reader renders")
}

#[test]
fn a_letter_like_follower_keeps_the_delimiter_spelling() {
    for follower in ["é", "中", "\u{200b}", "😀", "!", " z"] {
        let source = format!("x */a/*{follower} y\n");
        assert_eq!(
            carve::to_markdown(&source),
            format!("x ***a***{follower} y\n"),
            "follower {follower:?}",
        );
    }
}

#[test]
fn a_letter_like_follower_reads_back_as_both_emphases() {
    for follower in ["é", "中", "\u{200b}"] {
        let html = readback(&format!("x */a/*{follower} y\n"));
        assert!(
            html.contains("<em><strong>a</strong></em>")
                || html.contains("<strong><em>a</em></strong>"),
            "follower {follower:?}: {html}",
        );
    }
}

#[test]
fn a_letter_like_leader_keeps_the_delimiter_spelling() {
    assert_eq!(carve::to_markdown("é*/a/* y\n"), "é***a*** y\n");
}

#[test]
fn the_italic_around_a_strong_is_unchanged() {
    assert_eq!(carve::to_markdown("x /*a*/é y\n"), "x ***a***é y\n");
}

#[test]
fn literal_stars_around_an_italic_stay_escaped() {
    assert_eq!(carve::to_markdown("x */a/*a y\n"), "x \\**a*\\*a y\n");
}

#[test]
fn punctuation_content_against_a_letter_still_falls_back() {
    assert_eq!(
        carve::to_markdown("x */!/*é y\n"),
        "x <strong>*!*</strong>é y\n"
    );
}

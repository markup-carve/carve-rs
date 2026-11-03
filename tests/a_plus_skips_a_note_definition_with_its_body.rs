//! A `+` skips a note definition and its folded body as one invisible
//! construct (CARVE-P9-067): the body stays the note's, an indented `+` in it
//! is a letter, and the next visible block attaches (carve#5034).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_skipped_note_keeps_its_indented_plus_and_the_quote_attaches() {
    let out = html("[^a]: +\n[^n]: a\n +\npara\n> q\n\nsee[^n] [^a]\n");
    assert!(out.starts_with("<p>see"), "{out}");
    assert!(out.contains("<p>a\n+\npara<a href=\"#fnref1\""), "{out}");
    assert!(
        out.contains("<li id=\"fn2\">\n      <blockquote><p>q</p></blockquote>"),
        "{out}"
    );
}

#[test]
fn a_skipped_note_keeps_an_indented_text_line() {
    let out = html("[^a]: +\n[^n]: a\n x\npara\n> q\n\nsee[^n] [^a]\n");
    assert!(out.contains("<p>a\nx\npara<a href=\"#fnref1\""), "{out}");
    assert!(
        out.contains("<li id=\"fn2\">\n      <blockquote><p>q</p></blockquote>"),
        "{out}"
    );
}

#[test]
fn a_list_host_gives_the_skipped_note_its_body() {
    let out = html("- +\n[^n]: a\n +\npara\n> q\n\nsee[^n]\n");
    assert!(
        out.starts_with("<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>"),
        "{out}"
    );
    assert!(out.contains("<p>a\n+\npara<a href=\"#fnref1\""), "{out}");
}

//! carve-rs#3621: a definition term in a footnote body takes a lazy line as a
//! paragraph does, and a list marker below it still ends the note (corpus 855).

fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains(source: &str, expected: &str) {
    let out = html(source);
    assert!(out.contains(expected), "{source:?}: {out}");
}

#[test]
fn a_line_below_a_note_term_ends_the_note_in_every_host_shape() {
    // A term is one line (carve#4861), so it leaves the note no paragraph.
    for (source, term) in [
        ("[^f]: :: t\nx\n\n[^f]\n", "<p>x</p> <p><a id=\"fnref1\""),
        ("[^f]: [^g]: :: t\nx\n\n[^g] [^f]\n", "<p>x</p> <p><a id=\"fnref1\""),
        ("[^f]: - :: t\nx\n\n[^f]\n", "<p>x</p> <p><a id=\"fnref1\""),
        (
            "[^f]: [^g]: - :: t\n      : d\n\n[^g] [^f]\n",
            "<dt>t</dt> </dl> </li> </ul>",
        ),
        ("[^f]: a\n\n      :: t\nx\n\n[^f]\n", "<p>x</p> <p><a id=\"fnref1\""),
    ] {
        contains(source, term);
        contains(source, "<dt>t</dt>");
    }
}

#[test]
fn a_line_below_a_note_term_is_paragraph_text_before_a_marker() {
    assert!(
        html("[^n]: :: t\nmore\n- x\n\n[^n]\n").starts_with("<p>more - x</p>"),
    );
}

#[test]
fn a_marker_below_a_note_term_ends_the_note() {
    let out = html("[^n]: :: t\n- x\n\n[^n]\n");
    assert!(out.starts_with("<ul> <li>x</li> </ul>"), "{out}");
}

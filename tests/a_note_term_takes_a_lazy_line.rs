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
fn a_note_term_takes_a_lazy_line_in_every_host_shape() {
    for (source, term) in [
        ("[^f]: :: t\nx\n\n[^f]\n", "<dt>t x</dt>"),
        ("[^f]: [^g]: :: t\nx\n\n[^g] [^f]\n", "<dt>t x</dt>"),
        ("[^f]: - :: t\nx\n\n[^f]\n", "<dt>t x</dt>"),
        (
            "[^f]: [^g]: - :: t\n      : d\n\n[^g] [^f]\n",
            "<dt>t : d</dt>",
        ),
        ("[^f]: a\n\n      :: t\nx\n\n[^f]\n", "<dt>t x</dt>"),
    ] {
        contains(source, term);
    }
}

#[test]
fn a_marker_below_a_note_term_ends_the_note() {
    for source in [
        "[^n]: :: t\n- x\n\n[^n]\n",
        "[^n]: :: t\nmore\n- x\n\n[^n]\n",
    ] {
        let out = html(source);
        assert!(
            out.starts_with("<ul> <li>x</li> </ul>"),
            "{source:?}: {out}"
        );
    }
}

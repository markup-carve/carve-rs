#[test]
fn literal_backslashes_follow_the_reparse_rule() {
    for source in ["\\a and \\« and a\\b\n", "a\\\\b\n", "a\\!\n", "a\\\n"] {
        let written = carve::to_carve(source);
        assert_eq!(written, source, "{source:?}");
        assert_eq!(carve::to_carve(&written), written);
    }
}

#[test]
fn padded_table_cell_drops_an_idle_marker_escape() {
    let source = "|\\= a |\n";
    let expected = "| = a |\n";
    assert_eq!(carve::to_carve(source), expected);
    assert_eq!(carve::to_carve(expected), expected);
}

#[test]
fn every_backslash_follower_uses_reparse_in_inline_hosts() {
    let followers: Vec<char> = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~ \t\naZ0é中"
        .chars()
        .collect();
    type InlineHost = (&'static str, fn(&str) -> String);
    let hosts: [InlineHost; 6] = [
        ("paragraph", |text| format!("x{text}y\n")),
        ("emphasis", |text| format!("/x{text}y/\n")),
        ("link", |text| format!("[x{text}y](url)\n")),
        ("table", |text| format!("| x{text}y |\n")),
        ("heading", |text| format!("# x{text}y\n")),
        ("list", |text| format!("- x{text}y\n")),
    ];
    for (host, wrap) in hosts {
        for follower in followers
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::once(None))
        {
            for slashes in ["\\", "\\\\"] {
                let value = format!("{slashes}{}", follower.unwrap_or_default());
                let source = wrap(&value);
                let written = carve::to_carve(&source);
                assert_eq!(
                    carve::to_html(&written),
                    carve::to_html(&source),
                    "{host}, follower {follower:?}, {slashes:?}: {written:?}"
                );
                assert_eq!(
                    carve::to_carve(&written),
                    written,
                    "{host}, follower {follower:?}, {slashes:?}"
                );
            }
        }
    }
}

#[test]
fn bare_backslash_before_space_does_not_become_nonbreaking_space() {
    let source = "a\\\\ b\n";
    let written = carve::to_carve(source);
    assert_eq!(written, source);
    assert_eq!(carve::to_html(&written), carve::to_html(source));
}

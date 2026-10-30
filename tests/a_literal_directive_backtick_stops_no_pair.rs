//! A backtick inside a literal directive opens and closes nothing
//! (CARVE-P6-001, carve#4296), for bracket and braced pairs too.

#[test]
fn a_pair_around_the_directive_still_forms() {
    for (source, expected) in [
        ("{+{{ `@lines:1 }}+}\n", "<p><ins>{{ `@lines:1 }}</ins></p>"),
        ("{-{{ `@lines:1 }}-}\n", "<p><del>{{ `@lines:1 }}</del></p>"),
        (
            "{={{ `@lines:1 }}=}\n",
            "<p><mark>{{ `@lines:1 }}</mark></p>",
        ),
        (
            "[{{ `@lines:1 }}](u)\n",
            "<p><a href=\"u\">{{ `@lines:1 }}</a></p>",
        ),
        (
            "[*{{ `@lines:1 }}*](u)\n",
            "<p><a href=\"u\"><strong>{{ `@lines:1 }}</strong></a></p>",
        ),
        (
            "{~{{ `@lines:1 }}~>x~}\n",
            "<p><del>{{ `@lines:1 }}</del><ins>x</ins></p>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim_end(), expected, "{source}");
    }
}

#[test]
fn a_span_outside_the_directive_keeps_its_own_backticks() {
    assert_eq!(
        carve::to_html("{+`a`+}\n").trim_end(),
        "<p><ins><code>a</code></ins></p>"
    );
}

#[test]
fn an_attribute_candidate_before_the_pair_changes_nothing() {
    let out = carve::to_html("*a*{.c} {+{{ `@lines:1 }}+}\n");
    assert!(out.contains("<ins>{{ `@lines:1 }}</ins>"), "{out}");
}

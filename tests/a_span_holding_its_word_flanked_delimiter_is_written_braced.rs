//! An emphasis whose text holds its own delimiter between two word characters
//! is written in the forced form, not with an escaped delimiter: the bare span
//! would not form (PART 9 §9 R2), and the ruling on markup-carve/carve#4810
//! picks the braced spelling (markup-carve/carve#4967).

fn carve(src: &str) -> String {
    carve::to_carve(src)
}

fn assert_written(src: &str, expected: &str) {
    let out = carve(src);
    assert_eq!(out, expected, "fmt of {src:?}");
    assert_eq!(carve::to_html(&out), carve::to_html(src), "html of {src:?}");
    assert_eq!(carve(&out), out, "fmt is not settled for {src:?}");
}

#[test]
fn an_italic_holding_a_path_is_braced() {
    assert_written("{/usr/local/}\n", "{/usr/local/}\n");
    assert_written("x{/a/b/}y\n", "x{/a/b/}y\n");
    assert_written("{/1/2/} {/é/x/}\n", "{/1/2/} {/é/x/}\n");
}

#[test]
fn the_corpus_01_emphasis_13_fixture() {
    assert_written(
        "{/a/b/} and {/italic *bold*/}\n",
        "{/a/b/} and /italic *bold*/\n",
    );
}

#[test]
fn every_bare_kind_is_braced_the_same_way() {
    assert_written("{*a*b*}\n", "{*a*b*}\n");
    assert_written("{_my_var_}\n", "{_my_var_}\n");
    assert_written("{~a~b~}\n", "{~a~b~}\n");
    assert_written("{=a=b=}\n", "{=a=b=}\n");
}

#[test]
fn attributes_stay_outside_the_braces() {
    assert_written("{/a/b/}{.c}\n", "{/a/b/}{.c}\n");
}

#[test]
fn a_slash_that_is_not_word_flanked_is_still_escaped() {
    assert_written("{/a/ b/}\n", "/a\\/ b/\n");
}

#[test]
fn an_escaped_slash_in_the_source_stays_escaped() {
    assert_written("/a\\/b/\n", "/a\\/b/\n");
    assert_written("{/a\\/b c/d/}\n", "{/a\\/b c/d/}\n");
}

#[test]
fn a_mention_name_does_not_brace_its_span() {
    assert_written("_@a_b_\n", "_@a_b_\n");
}

#[test]
fn spans_without_an_inner_delimiter_stay_bare() {
    assert_written("/etc/ and _a_\n", "/etc/ and _a_\n");
}

#[test]
fn a_braced_comment_beside_the_delimiter_counts_as_nothing() {
    assert_written("{/a{% c %}/b/}\n", "{/a{% c %}/b/}\n");
    assert_written("{/a/{% c %}b/}\n", "{/a/{% c %}b/}\n");
    assert_written("{/a{% c %}/ b/}\n", "/a{% c %}\\/ b/\n");
}

#[test]
fn a_delimiter_after_a_mention_or_tag_name_is_word_flanked() {
    assert_written("{/@a/1/}\n", "{/@a/1/}\n");
    assert_written("{*@a-local*a*}\n", "{*@a-local*a*}\n");
    assert_written("{/#t/b/}\n", "{/#t/b/}\n");
}

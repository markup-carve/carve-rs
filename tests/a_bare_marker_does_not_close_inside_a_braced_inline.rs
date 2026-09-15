//! E2a, markup-carve/carve#2027: a bare closer is hidden inside a code span or
//! a braced inline, and nowhere else.

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn the_strike_stays_unopened_when_its_only_closer_is_inside_a_forced_span() {
    assert_eq!(html("~{/x~/}"), "<p>~<em>x~</em></p>");
}

#[test]
fn the_clause_example_answers_as_the_clause_states() {
    assert_eq!(html("~{/x/}{/y~/}"), "<p>~<em>x</em><em>y~</em></p>");
}

#[test]
fn a_closer_after_the_span_still_closes() {
    assert_eq!(
        html("~{/a/}{/b~/}c~"),
        "<p><s><em>a</em><em>b~</em>c</s></p>"
    );
}

#[test]
fn a_critic_insertion_is_hidden() {
    assert_eq!(html("~{+a~+}b~"), "<p><s><ins>a~</ins>b</s></p>");
}

#[test]
fn a_critic_substitution_is_hidden() {
    assert_eq!(
        html("~{~a~>b~~}c~"),
        "<p><s><del>a</del><ins>b~</ins>c</s></p>"
    );
}

#[test]
fn a_critic_comment_is_hidden() {
    assert_eq!(
        html("~{#a~#}b~"),
        "<p><s><span class=\"critic-comment\">a~</span>b</s></p>"
    );
}

#[test]
fn a_comment_whose_body_holds_an_apostrophe_is_hidden() {
    assert_eq!(html("~{% it's~ %}b~"), "<p><s>b</s></p>");
}

#[test]
fn a_forced_span_across_a_newline_is_hidden_because_the_parser_builds_one() {
    assert_eq!(html("~{/a\nb~/}"), "<p>~<em>a\nb~</em></p>");
}

#[test]
fn a_plain_brace_pair_is_not_hidden() {
    assert_eq!(html("~{y~}z~"), "<p><s>{y</s>}z~</p>");
}

#[test]
fn an_attribute_block_is_not_hidden() {
    assert_eq!(html("~x{.c~}y~"), "<p><s>x{.c</s>}y~</p>");
}

#[test]
fn a_critic_comment_opener_that_never_closes_as_one_is_not_hidden() {
    assert_eq!(
        html("~a{#x~}b~"),
        "<p><s>a{<span class=\"tag\"><strong>#x</strong></span></s>}b~</p>"
    );
}

#[test]
fn a_code_span_is_still_hidden() {
    assert_eq!(html("~`a~b`~"), "<p><s><code>a~b</code></s></p>");
}

#[test]
fn a_link_label_is_not_hidden() {
    assert_eq!(html("~[a~](b)"), "<p><s>[a</s>](b)</p>");
}

//! A `%%` line below a sub-list must not gain a blank line when something
//! visible follows: the blank stands in front of an invisible line and
//! loosens the list at the next sibling marker (carve#4904).

fn round_trips(src: &str) {
    let first = carve::to_carve(src);
    assert_eq!(
        carve::to_html(&first),
        carve::to_html(src),
        "HTML moved: {src:?} -> {first:?}"
    );
    assert_eq!(carve::to_carve(&first), first, "not a fixpoint: {src:?}");
}

#[test]
fn the_reported_shapes_keep_their_html() {
    for src in [
        "- o\n  - - s\n  %% c\n  - s\n",
        "- - s\n  %% c\n- s\n",
        "- - s\n%% c\n- s\n",
        "- o\n  - - s\n    %% c\n- t\n",
        "> - - s\n> %% c\n> - s\n",
        "- a\n  - s\n  %% c\n  p\n",
        "- a\n  - s\n  %% c\n  - t\n",
        "- :: t\n  : d\n    %% c\n- s\n",
        "- :: t\n  : d\n    %%%\n    c\n    %%%\n- s\n",
        "- o\n  - - s\n    - t\n      - u\n\n     %% c\n    :: u\n    : v\n",
    ] {
        round_trips(src);
    }
}

#[test]
fn a_following_sibling_takes_no_separator() {
    assert_eq!(
        carve::to_carve("- - s\n  %% c\n- s\n"),
        "- - s\n  %% c\n- s\n"
    );
    assert_eq!(
        carve::to_carve("- o\n  - - s\n  %% c\n  - s\n"),
        "- o\n  - - s\n  %% c\n  - s\n"
    );
}

#[test]
fn a_description_s_trailing_comment_takes_no_separator_before_a_sibling() {
    assert_eq!(
        carve::to_carve("- :: t\n  : d\n- s\n\n%% c\n"),
        "- :: t\n  : d\n- s\n\n%% c\n"
    );
}

#[test]
fn a_trailing_comment_keeps_its_separator() {
    assert_eq!(carve::to_carve("- - d\n  %% q\n"), "- - d\n\n  %% q\n");
}

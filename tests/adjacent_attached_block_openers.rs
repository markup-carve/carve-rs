fn fmt(source: &str) -> String {
    carve::to_carve(source)
}

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn adjacent_block_openers_stay_separate() {
    for source in [
        "- x\n+\n> q\n+\n> q\n",
        "- x\n+\n| a |\n|---|\n| b |\n+\n| a |\n|---|\n| b |\n",
        "- x\n+\n::: |\na\n:::\n+\n::: |\nb\n:::\n",
    ] {
        assert_eq!(html(&fmt(source)), html(source));
        assert_eq!(fmt(&fmt(source)), fmt(source));
    }
}

#[test]
fn an_isolated_block_opener_keeps_the_explicit_boundary() {
    assert_eq!(fmt("- x\n+\n> q\n"), "- x\n+\n> q\n");
}

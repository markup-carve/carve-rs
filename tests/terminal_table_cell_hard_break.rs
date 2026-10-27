#[test]
fn terminal_breaks_survive_canonical_writing() {
    for source in [
        "| a\\ | b |\n",
        "| a\\ | b\\ |\n",
        "|= head\\ | data |\n",
        "| a\\|b\\ | c\\|d |\n",
    ] {
        assert_eq!(carve::to_carve(source), source);
    }
}

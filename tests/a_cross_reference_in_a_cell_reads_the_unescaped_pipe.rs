use carve::to_html;

#[test]
fn a_cross_reference_in_a_cell_reads_the_unescaped_pipe() {
    assert!(
        to_html("|</#\\|>|\n").contains("<td>&lt;/#|&gt;</td>"),
        "{}",
        to_html("|</#\\|>|\n")
    );
}

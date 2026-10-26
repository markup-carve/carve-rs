#[test]
fn a_table_ends_the_paragraph_that_could_retain_a_flush_continuation() {
    let source = "- a\n  ::: box\n    | x | y |\n+ c";
    let expected = "<ul>\n  <li>a\n    <div class=\"box\">\n      <table>\n        <tbody>\n          <tr><td>x</td><td>y</td></tr>\n        </tbody>\n      </table>\n    </div>\n  </li>\n</ul>\n<p>+ c</p>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

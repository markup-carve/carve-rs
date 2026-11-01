use carve::to_html;

#[test]
fn authored_spans_cannot_change_the_table_grid() {
    for (source, expected) in [
        (
            "|{rowspan=2} a | b |\n| c | d |\n",
            "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>c</td><td>d</td></tr>\n  </tbody>\n</table>",
        ),
        (
            "|{colspan=2} a | b |\n",
            "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>",
        ),
        (
            "|{RowSpan=9 colspan=7 .x} a |\n| ^ |\n",
            "<table>\n  <tbody>\n    <tr><td rowspan=\"2\" class=\"x\">a</td></tr>\n    <tr></tr>\n  </tbody>\n</table>",
        ),
        (
            "|>{align=left} a |\n",
            "<table>\n  <tbody>\n    <tr><td align=\"left\" style=\"text-align: right;\">a</td></tr>\n  </tbody>\n</table>",
        ),
    ] {
        assert_eq!(to_html(source), expected, "{source:?}");
    }
}

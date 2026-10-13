//! A `+` continuation on a span marker joins the cell the T5 walk resolves the
//! marker into; a blocked marker renders empty and keeps none of it
//! (markup-carve/carve#3097). Expected output is the spec checker's.

#[test]
fn caret_after_the_origin_row() {
    assert_eq!(
        carve::to_html("| a | b |\n| ^ | c |\n+ more | d |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\">a more</td><td>b</td></tr>\n    <tr><td>c d</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn continuation_before_the_caret_row() {
    assert_eq!(
        carve::to_html("| a | b |\n+ more | d |\n| ^ | c |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\">a more</td><td>b d</td></tr>\n    <tr><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn chain_of_carets() {
    assert_eq!(
        carve::to_html("| a | b |\n| ^ | c |\n| ^ | e |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"3\">a x</td><td>b</td></tr>\n    <tr><td>c</td></tr>\n    <tr><td>e y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn colspan_marker() {
    assert_eq!(
        carve::to_html("| a | < |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td colspan=\"2\">a x y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn rowspan_and_colspan_block() {
    assert_eq!(
        carve::to_html("| a | < | c |\n| ^ | ^ | d |\n+ x | y | z |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\" colspan=\"2\">a x y</td><td>c</td></tr>\n    <tr><td>d z</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn gfm_head_rowspan() {
    assert_eq!(
        carve::to_html("| A |\n|---|\n| ^ |\n+ x |\n").trim(),
        "<table>\n  <tbody>\n    <tr><th scope=\"col\" rowspan=\"2\">A x</th></tr>\n    <tr></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocked_colspan_marker() {
    assert_eq!(
        carve::to_html("| a | b |\n| < | c |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td></td><td>c y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocked_caret_in_the_first_row() {
    assert_eq!(
        carve::to_html("| ^ | b |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td></td><td>b y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn two_origins_in_one_row() {
    assert_eq!(
        carve::to_html("| a | b | c |\n| ^ | d | e |\n| f | ^ | g |\n+ x | y | z |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\">a</td><td>b</td><td>c</td></tr>\n    <tr><td rowspan=\"2\">d y</td><td>e</td></tr>\n    <tr><td>f x</td><td>g z</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocked_caret_under_a_wide_cell() {
    assert_eq!(
        carve::to_html("| a | < |\n| ^ | b |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td colspan=\"2\">a</td></tr>\n    <tr><td></td><td>b y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn two_continuation_lines_on_one_caret() {
    assert_eq!(
        carve::to_html("|= h |= i |\n| a | b |\n| ^ | c |\n+ more | |\n+ again | d |\n").trim(),
        "<table>\n  <thead>\n    <tr><th scope=\"col\">h</th><th scope=\"col\">i</th></tr>\n  </thead>\n  <tbody>\n    <tr><td rowspan=\"2\">a more again</td><td>b</td></tr>\n    <tr><td>c d</td></tr>\n  </tbody>\n</table>"
    );
}

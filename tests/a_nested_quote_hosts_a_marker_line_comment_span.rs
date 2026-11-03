//! A `%%%` span on a nested quote's item or description lead is that host's
//! (PART 9 §28): the outer quote must not read the host's closer as a new
//! opener (carve#5026, corpus family 944).

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn a_follower_after_a_quoted_span_leaves_both_quotes() {
    assert_eq!(
        html("> > - %%%\n> > %%%\n> > %%%\n> > x\nt\n"),
        "<blockquote>\n  <blockquote>\n    <ul>\n      <li></li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>t</p>"
    );
}

#[test]
fn a_marker_follower_opens_a_document_list() {
    let out = html("> > > 1. %%%%\n> > > %%%%\n> > > %%%%\n> > > x\n- t\n");
    assert!(
        out.ends_with("</blockquote>\n<ul>\n  <li>t</li>\n</ul>"),
        "{out}"
    );
}

#[test]
fn a_description_lead_span_keeps_its_follower() {
    assert_eq!(
        html("> :: d\n> : %%%\n> %%%\n> %%%\n> x\n: t\n"),
        "<blockquote>\n  <dl>\n    <dt>d</dt>\n    <dd></dd>\n  </dl>\n</blockquote>\n<p>: t</p>"
    );
}

#[test]
fn a_lazy_line_folds_into_the_nested_quote_after_the_hosts_closer() {
    let out = html("> > - %%%\n> > %%%\n> > x\nt\n");
    assert!(
        out.contains("<p>x\nt</p>\n  </blockquote>\n</blockquote>"),
        "{out}"
    );
}

#[test]
fn a_closed_quote_span_lets_the_follower_fold() {
    let out = html("> > - %%%\n> > %%%\n> > %%%\n> > %%%\n> > x\nt\n");
    assert!(
        out.contains("<p>x\nt</p>\n  </blockquote>\n</blockquote>"),
        "{out}"
    );
}

#[test]
fn a_description_marker_folded_into_a_paragraph_hosts_no_span() {
    let out = html("> :: d\n> p\n> : %%%\n> %%%\n> x\nt\n");
    assert!(out.ends_with("</blockquote>\n<p>t</p>"), "{out}");
}

#[test]
fn a_line_that_leaves_the_nested_quote_ends_its_hosted_span() {
    let out = html("> > - %%%\n> x\n> > %%%\n> > y\nz\n");
    assert!(out.ends_with("</blockquote>\n<p>z</p>"), "{out}");
}

#[test]
fn a_line_that_leaves_the_nested_quote_ends_its_span() {
    let out = html("> > a\n> > %%%\n> x\n> > %%%\n> > y\nt\n");
    assert!(out.ends_with("</blockquote>\n<p>t</p>"), "{out}");
}

//! An attribute run that attaches to nothing is text, so a closer inside its
//! quoted value closes (carve-rs#3485). The label half of a reference pair takes
//! no destination (carve-rs#3486). Editorial text feeds the heading id
//! (carve-rs#3488).

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn a_braced_closer_inside_an_unattached_attribute_run_closes() {
    assert_eq!(
        html("{*x{title=\"*}\"} b*}\n"),
        "<p><strong>x{title=“</strong>”} b*}</p>"
    );
    assert_eq!(
        html("{/a {.k title=\"/}\"} b/}\n"),
        "<p><em>a {.k title=“</em>”} b/}</p>"
    );
    assert_eq!(
        html("{*a{#c#}{title=\"*}\"} b*}\n"),
        "<p><strong>a<span class=\"critic-comment\">c</span>{title=“</strong>”} b*}</p>"
    );
}

#[test]
fn an_attached_attribute_run_stays_opaque() {
    assert_eq!(
        html("{*`c`{title=\"*}\"} b*}\n"),
        "<p><strong><code title=\"*}\">c</code> b</strong></p>"
    );
    assert_eq!(
        html("{*a{+i+}{title=\"*}\"} b*}\n"),
        "<p><strong>a<ins title=\"*}\">i</ins> b</strong></p>"
    );
}

#[test]
fn a_reference_label_takes_no_destination() {
    assert_eq!(
        html("*[b][a](u*) c*\n"),
        "<p><strong>[b][a](u</strong>) c*</p>"
    );
    assert_eq!(
        html("/*[b][a](u*/) c*/\n"),
        "<p><strong><em>[b][a](u</em></strong>) c*/</p>"
    );
    assert_eq!(
        html("*![i][a](u*) c*\n"),
        "<p><strong>![i][a](u</strong>) c*</p>"
    );
}

#[test]
fn a_third_glued_bracket_is_link_text_again() {
    assert_eq!(
        html("*[b][a][c](u*) c*\n"),
        "<p><strong>[b][a]<a href=\"u*\">c</a> c</strong></p>"
    );
    assert_eq!(
        html("*[^n][a](u*) c*\n"),
        "<p><strong>[^n]<a href=\"u*\">a</a> c</strong></p>"
    );
}

#[test]
fn editorial_text_feeds_the_heading_id() {
    assert!(html("# {+[a](u) b+}\n").starts_with("<section id=\"a-b\">"));
    assert!(html("# x {-a-}\n").starts_with("<section id=\"x-a\">"));
    assert!(html("# x {~a~>b~}\n").starts_with("<section id=\"x-ab\">"));
    assert_eq!(
        html("[x {+a+}][]\n\n# x {+a+}\n").lines().next().unwrap(),
        "<p><a href=\"#x-a\">x <ins>a</ins></a></p>"
    );
}

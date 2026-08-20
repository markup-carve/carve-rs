//! A BOOLEAN ATTRIBUTE DOES NOT START WITH AN UNDERSCORE
//! (markup-carve/carve#1450).
//!
//! An identifier may, so `{_x_}` was two constructs at once: the boolean
//! attribute `_x_`, and a forced underline. Alone on a line the attribute
//! reading won -- the block below came out as `<p _x_="">`, and with no block
//! below the line rendered NOTHING, five characters kept in the source and gone
//! from the output.
//!
//! The BARE form gives the collision up. HTML has no underscore-first boolean
//! attribute to lose, and every other attribute form keeps its leading
//! underscore, because none of them ends `_}`.

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_lone_braced_pair_is_an_underline() {
    assert_eq!(html("{_x_}\n"), "<p><u>x</u></p>");
    assert_eq!(html("{_x_}\npara\n"), "<p><u>x</u>\npara</p>");
}

#[test]
fn it_is_an_underline_mid_line_too() {
    assert_eq!(html("{_x_} y\n"), "<p><u>x</u> y</p>");
    assert_eq!(html("y {_x_}\n"), "<p>y <u>x</u></p>");
}

#[test]
fn a_bare_underscore_first_word_is_text() {
    // It has no underline reading either -- it does not end `_}` -- so it
    // renders literally rather than becoming something else.
    assert_eq!(html("{_foo}\npara\n"), "<p>{_foo}\npara</p>");
    assert_eq!(html("[x]{_u}\n"), "<p>[x]{_u}</p>");
}

#[test]
fn every_other_attribute_form_keeps_its_underscore() {
    assert_eq!(
        html("{#_id ._c _k=1 _=\"on click\"}\npara\n"),
        "<p id=\"_id\" class=\"_c\" _k=\"1\" _=\"on click\">para</p>"
    );
    assert_eq!(html("[x]{#_u}\n"), "<p><span id=\"_u\">x</span></p>");
    assert_eq!(html("[x]{._u}\n"), "<p><span class=\"_u\">x</span></p>");
    assert_eq!(html("[x]{_u=1}\n"), "<p><span _u=\"1\">x</span></p>");
}

#[test]
fn an_ordinary_boolean_attribute_still_reads() {
    assert_eq!(html("{disabled}\npara\n"), "<p disabled=\"\">para</p>");
    assert_eq!(html("[x]{kbd}\n"), "<p><kbd>x</kbd></p>");
}

#[test]
fn the_writer_keeps_the_empty_value_on_an_underscore_name() {
    // PART 11 §6c shortens a value-less attribute to its bare name and cannot
    // here: `{_u}` is text and `{_x_}` is an underline, either way a document
    // the writer changed, which §1 forbids.
    assert_eq!(carve::to_carve("[x]{_u=\"\"}\n"), "[x]{_u=\"\"}\n");
    assert_eq!(
        html(&carve::to_carve("[x]{_u=\"\"}\n")),
        html("[x]{_u=\"\"}\n")
    );
    // An ordinary name still shortens.
    assert_eq!(carve::to_carve("[x]{kbd=\"\"}\n"), "[x]{kbd}\n");
}

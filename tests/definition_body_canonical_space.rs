use carve::{to_carve, to_html};

#[test]
fn one_space_is_accepted_and_canonical() {
    let source = ":: x\n: y\n";
    assert_eq!(to_carve(source), source);
}

#[test]
fn a_wider_separator_is_narrowed_with_its_body() {
    let source = ":: x\n:  y\n\n   > q\n";
    let canonical = ":: x\n: y\n\n  > q\n";
    assert_eq!(to_carve(source), canonical);
    assert_eq!(to_html(source), to_html(canonical));
}

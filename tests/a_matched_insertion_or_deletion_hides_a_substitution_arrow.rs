//! A matched `{+ +}` or `{- -}` inside a substitution is an E2a boundary (PART 9
//! section 22), so a `~>` or `~}` inside it belongs to that frame and does not
//! split the substitution (carve-rs#3498, corpus 772).

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

#[test]
fn an_arrow_inside_a_nested_frame_is_frame_text() {
    assert_eq!(
        html("{~{+b~>c+}~>a~}\n"),
        "<p><del><ins>b~&gt;c</ins></del><ins>a</ins></p>"
    );
    assert_eq!(html("{~{-b~>c-}~}\n"), "<p><s><del>b~&gt;c</del></s></p>");
    assert_eq!(
        html("{~a{-b~>c-}~>d~}\n"),
        "<p><del>a<del>b~&gt;c</del></del><ins>d</ins></p>"
    );
    assert_eq!(
        html("{~{+b~}c+}~>a~}\n"),
        "<p><del><ins>b~}c</ins></del><ins>a</ins></p>"
    );
    assert_eq!(
        html("[{~{+b~>c+}~>a~}](u)\n"),
        "<p><a href=\"u\"><del><ins>b~&gt;c</ins></del><ins>a</ins></a></p>"
    );
}

#[test]
fn an_unclosed_frame_imposes_no_boundary() {
    assert_eq!(
        html("{~{+b~>c~>a~}\n"),
        "<p><del>{+b</del><ins>c~&gt;a</ins></p>"
    );
}

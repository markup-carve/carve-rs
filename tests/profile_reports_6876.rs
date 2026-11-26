//! carve#6876: a comment of every form outside the profile is reported once at
//! its own position (C2-H-163, C2-H-165, C2-A-151); an `abbreviation_def` never
//! is (C2-H-178).

use carve::api::{self, Options, Profile};

/// `(start offset, construct)` of each profile report.
fn reports(input: &str, profile: Profile) -> Vec<(usize, String)> {
    let o = Options {
        profile,
        ..Options::default()
    };
    let r = api::render_with_diagnostics(input, &o).unwrap();
    r.diagnostics
        .iter()
        .filter(|d| d.code == "profile-construct-degraded")
        .map(|d| {
            let start = d.range.as_ref().map_or(usize::MAX, |r| r.start_offset);
            let construct = d.message.rsplit(": ").next().unwrap_or("");
            (start, construct.to_string())
        })
        .collect()
}

fn comment(start: usize) -> (usize, String) {
    (start, "comment.".to_string())
}

#[test]
fn a_comment_on_its_own_line_inside_a_paragraph_is_reported() {
    assert_eq!(reports("a\n%% c\nb\n", Profile::Comment), [comment(2)]);
}

#[test]
fn a_paragraph_of_braced_comments_reports_each() {
    assert_eq!(
        reports("{% c %} {% d %}\n", Profile::Minimal),
        [comment(0), comment(8)]
    );
}

#[test]
fn a_comment_inside_a_degraded_node_is_not_reported_again() {
    let got = reports("# h {% c %} i\n", Profile::Comment);
    assert_eq!(got, [(0, "heading.".to_string())]);
}

#[test]
fn comments_inside_the_profile_are_not_reported() {
    assert!(reports("x %% c\n\nb {% c %} d\n", Profile::Article).is_empty());
}

#[test]
fn an_abbreviation_definition_is_not_reported_under_comment() {
    assert!(reports("*[B]: bee\n\nA b.\n", Profile::Comment).is_empty());
}

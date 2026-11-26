//! carve#6834: an empty element counts in a ranged comment's range exactly
//! when the import keeps a node for it (C2-M-113, C2-M-069).

use carve::api::{self, ImportOptions};

const START: &str = "<p><span class=\"comment-start\" data-comment-id=\"1\">n</span>";
const END: &str = "<span class=\"comment-end\" data-comment-id=\"1\"></span>y</p>";
const LOST: &str = "Ranged comment read as a point comment; its range is lost";

fn import(inner: &str) -> (String, Vec<String>, Vec<String>) {
    let html = format!("{START}{inner}{END}");
    let got = api::import_document(&html, &ImportOptions::default()).expect("import");
    let mut codes: Vec<String> = got.diagnostics.iter().map(|d| d.code.clone()).collect();
    codes.sort();
    let messages = got.diagnostics.iter().map(|d| d.message.clone()).collect();
    (got.source, codes, messages)
}

const CASES: &[(&str, &str, &[&str], bool)] = &[
    (
        "<em title=\"x\"></em>",
        "{#n#}y\n",
        &["import-structure-repaired"],
        false,
    ),
    (
        "<span onclick=\"x\"></span>",
        "{#n#}y\n",
        &["import-attribute-dropped"],
        false,
    ),
    ("<bdi></bdi>", "{#n#}y\n", &["import-element-mapped"], false),
    (
        "<small></small>",
        "{#n#}y\n",
        &["import-element-mapped"],
        false,
    ),
    ("<q></q>", "{#n#}y\n", &["import-element-mapped"], false),
    (
        "<strong id=\"u\"></strong>",
        "{#n#}y\n",
        &["import-structure-repaired"],
        false,
    ),
    (
        "<mark class=\"c\"></mark>",
        "{#n#}y\n",
        &["import-structure-repaired"],
        false,
    ),
    ("<sup lang=\"de\"></sup>", "{#n#}y\n", &[], false),
    ("<a href=\"u\"></a>", "{#n#}y\n", &[], false),
    (
        "<strong><em title=\"x\"></em></strong>",
        "{#n#}y\n",
        &["import-structure-repaired"],
        false,
    ),
    (
        "<a data-carve-autolink href=\"u\"><em></em></a>",
        "{#n#}y\n",
        &["import-carrier-ignored"],
        false,
    ),
    (
        "<strong>a<em title=\"x\"></em></strong>",
        "{#n#}{*a*}y\n",
        &["import-structure-repaired"],
        true,
    ),
    (
        "<em title=\"x\"></em>a",
        "{#n#}ay\n",
        &["import-structure-repaired"],
        true,
    ),
    (
        "a<em title=\"x\"></em>",
        "{#n#}ay\n",
        &["import-structure-repaired"],
        true,
    ),
    ("<a id=\"u\"></a>", "{#n#}[]{#u}y\n", &[], true),
    (
        "<span title=\"x\"></span>",
        "{#n#}[]{title=x}y\n",
        &[],
        true,
    ),
    (
        "<span title=\"t\"><em></em></span>",
        "{#n#}[]{title=t}y\n",
        &[],
        true,
    ),
    (
        "<span title=\"t\"><em><b></b></em></span>",
        "{#n#}[]{title=t}y\n",
        &[],
        true,
    ),
    (
        "<cite title=\"x\"></cite>",
        "{#n#}[]{title=x}y\n",
        &["import-element-mapped"],
        true,
    ),
];

#[test]
fn range_counts_only_kept_nodes() {
    for &(inner, source, codes, lost) in CASES {
        let (src, got, messages) = import(inner);
        assert_eq!(src, source, "{inner}");
        let mut want: Vec<String> = codes.iter().map(ToString::to_string).collect();
        if lost {
            want.push("import-structure-repaired".into());
        }
        want.sort();
        assert_eq!(got, want, "{inner}");
        assert_eq!(messages.iter().any(|m| m == LOST), lost, "{inner}");
    }
}

#[test]
fn dropped_empty_element_leaves_one_report() {
    let (_, _, messages) = import("<em title=\"x\"></em>");
    assert_eq!(messages, ["Empty HTML em with attributes dropped"]);
}

/// The probes read in document order, so a carrier that repeats an earlier
/// ordinal is ignored there as in the import.
#[test]
fn repeated_snippet_ordinal_is_ignored_in_the_probe() {
    for inside in [
        "<em data-carve-snippet=\"1:inline:$v\"><i></i></em>",
        "<span data-carve-snippet=\"1:inline:$v\"></span>",
    ] {
        let html = format!(
            "<p>A <span data-carve-snippet=\"1:inline:$v\">X</span></p>{START}{inside}{END}"
        );
        let got = api::import_document(&html, &ImportOptions::default()).expect("import");
        assert!(got.source.ends_with("{#n#}y\n"), "{}", got.source);
        assert!(
            got.diagnostics.iter().all(|d| d.message != LOST),
            "{inside}"
        );
    }
}

/// A `<code>` is read from its text, so a carrier inside it records no
/// ordinal, and a later carrier with that ordinal stands in the range.
#[test]
fn carrier_inside_code_records_no_ordinal() {
    let html = format!(
        "<p><code><span data-carve-snippet=\"1:inline:$v\">X</span></code></p>{START}{}{END}",
        "<span data-carve-snippet=\"1:inline:$v\"></span>",
    );
    let got = api::import_document(&html, &ImportOptions::default()).expect("import");
    assert!(got.source.ends_with("{#n#}{{ $v }}y\n"), "{}", got.source);
    assert!(got.diagnostics.iter().any(|d| d.message == LOST));
}

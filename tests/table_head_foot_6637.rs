//! C2-B-187, C2-W-136 (carve#6637): an empty `body-rows` ends the head above a
//! footer of header rows, and the writer and the HTML import spell that table.
use carve::api::{self, Carve, ImportOptions, LintOptions, Options, WriteOptions};

fn render(src: &str) -> String {
    api::render(src, &Options::default()).unwrap()
}

#[test]
fn empty_body_rows_ends_the_head_above_the_foot() {
    for (src, head, foot) in [
        ("{footer-rows=1 body-rows}\n|= A |\n|= F |\n", 1, 1),
        ("{footer-rows=2 body-rows}\n|= A |\n|= F |\n|= G |\n", 1, 2),
        ("{footer-rows=1 body-rows}\n|= A |\n|= B |\n|= F |\n", 2, 1),
        ("{footer-rows=2 body-rows}\n|= A |\n|= F |\n| G |\n", 1, 2),
        ("{body-rows=\"\" footer-rows=1}\n|= A |\n|= F |\n", 1, 1),
    ] {
        let html = render(src);
        let thead = html.split("</thead>").next().unwrap_or("");
        let tfoot = html.split("<tfoot>").nth(1).unwrap_or("");
        assert_eq!(thead.matches("<tr>").count(), head, "{src:?}\n{html}");
        assert_eq!(tfoot.matches("<tr>").count(), foot, "{src:?}\n{html}");
        assert!(!html.contains("<tbody>"), "{src:?}\n{html}");
        let codes: Vec<String> = api::lint(src, &LintOptions::default())
            .unwrap()
            .into_iter()
            .map(|e| e.code)
            .collect();
        assert!(
            !codes.iter().any(|c| c == "table-row-count-clamped"),
            "{src:?}: {codes:?}"
        );
    }
}

#[test]
fn head_of_c2_b_178_stays_without_an_empty_body_rows() {
    for src in [
        "{footer-rows=1}\n|= A |\n|= F |\n",
        "{footer-rows=2 body-rows}\n|= A |\n|= F |\n",
        "{footer-rows=1 body-rows=0}\n|= A |\n|= F |\n",
        "{footer-rows=1 body-rows body-header-rows=1}\n|= A |\n|= F |\n",
    ] {
        let html = render(src);
        assert!(!html.contains("<tfoot>"), "{src:?}\n{html}");
    }
}

#[test]
fn writes_the_empty_body_rows_and_reads_it_back() {
    let carve = Carve::new();
    for src in [
        "{footer-rows=1 body-rows}\n|= A |\n|= F |\n",
        "{footer-rows=2 body-rows}\n|= A |\n|= B |\n|= F |\n| G |\n",
        "{.t footer-rows=1 body-rows}\n|= A |= B |\n|= F |= G |\n^ Cap\n",
        "{footer-rows=1 body-rows}\n|= A |\n|= ^ |\n|= F |\n",
    ] {
        let tree = carve.parse(src, &Options::default()).unwrap();
        let written = carve
            .write_with_diagnostics(&tree, &WriteOptions::default())
            .unwrap();
        assert_eq!(written.output, src);
        assert!(
            written.diagnostics.is_empty(),
            "{src:?}: {:?}",
            written.diagnostics
        );
    }
    for src in [
        "{footer-rows=1}\n|= A |\n| F |\n",
        "{footer-rows=1}\n|= A |\n| b |\n|= F |\n",
    ] {
        let tree = carve.parse(src, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &WriteOptions::default()).unwrap(), src);
    }
}

#[test]
fn imports_a_head_and_header_cell_foot_without_a_body() {
    for (html, want) in [
        (
            "<table><thead><tr><th>A</th><th>B</th></tr></thead><tfoot><tr><th>F</th><th>G</th></tr></tfoot></table>",
            "{footer-rows=1 body-rows}\n|= A |= B |\n|= F |= G |\n",
        ),
        (
            "<table><tr><th>A</th><th>B</th></tr><tfoot><tr><th>F</th><th>G</th></tr></tfoot></table>",
            "{footer-rows=1 body-rows}\n|= A |= B |\n|= F |= G |\n",
        ),
        (
            "<table><thead><tr><th>A</th></tr><tr><th>B</th></tr></thead><tfoot><tr><th>F</th></tr><tr><td>G</td></tr></tfoot></table>",
            "{footer-rows=2 body-rows}\n|= A |\n|= B |\n|= F |\n| G |\n",
        ),
    ] {
        let got = api::import_document(html, &ImportOptions::default()).unwrap();
        assert_eq!(got.source, want, "{html}");
        assert!(got.diagnostics.is_empty(), "{html}: {:?}", got.diagnostics);
    }
}

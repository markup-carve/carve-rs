//! A promoted image's block attribute run is a dropped block attribute in
//! default Markdown; attributes glued to the image follow the inline rule
//! (CARVE-P2-024, markup-carve/carve#3975).

use carve::{to_markdown_with_report, CheckedRenderOptions};

fn dropped(source: &str) -> (String, Vec<(usize, usize, usize)>) {
    let result = to_markdown_with_report(source, CheckedRenderOptions::default()).unwrap();
    let spans = result
        .losses
        .iter()
        .filter(|loss| loss.code == "markdown-attributes-dropped")
        .map(|loss| {
            let pos = loss.pos.as_ref().expect("a span");
            (pos.start_line, pos.start_column, pos.end_column)
        })
        .collect();
    (result.value, spans)
}

#[test]
fn a_block_run_merged_into_a_promoted_image_is_one_loss_at_the_image() {
    assert_eq!(
        dropped("{.outer k=one}\n![a](b){.inner k=two}\n"),
        ("![a](b)\n".to_string(), vec![(2, 1, 22)])
    );
    assert_eq!(
        dropped("{.o}\n![a][r]\n\n[r]: b.png\n"),
        ("![a](b.png)\n".to_string(), vec![(2, 1, 8)])
    );
}

#[test]
fn every_host_reports_the_run() {
    for source in [
        "{.outer}\n![alt](img.png)\n",
        "- {#i}\n  ![a](b.png){.x}\n",
        "> {k=v}\n> ![a](b.png)\n",
        ":::\n{.o}\n![a](b.png)\n:::\n",
    ] {
        assert_eq!(dropped(source).1.len(), 1, "{source:?}");
    }
}

#[test]
fn glued_attributes_and_an_empty_run_report_nothing() {
    for source in [
        "![alt](img.png){.x}\n",
        "![a](u){k=\"{y}\"}\n",
        "![a][r]{.x}\n\n[r]: b.png\n",
        ":::\n![a](b.png){#g}\n:::\n",
        "{}\n![a](b.png){.x}\n",
        "a ![alt](i.png){.x} b\n",
    ] {
        assert_eq!(dropped(source).1, Vec::new(), "{source:?}");
    }
}

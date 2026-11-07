use carve::{decision_trace, parse, render_html};

fn html(source: &str) -> String {
    render_html(&parse(source)).unwrap()
}

#[test]
fn marker_line_attachment_trace_keeps_its_item_owner() {
    for (opener, family) in [("::: note", "div"), ("```", "code")] {
        let source = format!("- +\n{opener}\n\n  tail\n");
        let trace = decision_trace(&source);
        assert!(
            trace.contains(&format!(
                "\"line\":2,\"step\":\"S5\",\"owner\":\"doc/item:1/{family}:2\""
            )),
            "{source:?}: {trace}"
        );
        assert!(
            !trace.lines().find(|line| line.contains("\"line\":2")).unwrap().contains("\"closed\""),
            "{source:?}: {trace}"
        );
        assert!(trace.contains("\"line\":4,\"step\":\"S5\",\"owner\":\"doc/item:1\""));
        assert!(html(&source).contains("<p>tail</p>"));
    }
}

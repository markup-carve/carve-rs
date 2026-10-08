pub fn source(shape: &str, n: usize) -> String {
    if shape == "separators" {
        return format!("<p>Body<a href=\"#fn1\" role=\"doc-noteref\">1</a>.</p><section>{}<p id=\"fn1\">Note.</p></section><p>Tail.</p>", "<hr> \n<!--layout-->".repeat(n));
    }
    if shape == "duplicate-identities" {
        return format!(
            "<p>{}</p><section><p id=\"fn1\">Note.{}</p></section>",
            "<a id=\"r\" href=\"#fn1\" role=\"doc-noteref\">1</a>".repeat(n),
            "<a href=\"#r\" class=\"footnote-back\">back</a>".repeat(n)
        );
    }
    if shape == "deep-shared-wrapper" {
        let refs = (0..n)
            .map(|i| format!("<a href=\"#fn{i}\" role=\"doc-noteref\">1</a>"))
            .collect::<String>();
        let notes = (0..n)
            .map(|i| format!("<p id=\"fn{i}\">Note.</p>"))
            .collect::<String>();
        return format!(
            "<p>{refs}</p><section>{}{notes}{}</section><p>Tail.</p>",
            "<object>".repeat(n),
            "</object>".repeat(n)
        );
    }
    if shape == "nested-backlink-blocks" {
        let refs = (0..n)
            .map(|i| format!("<a id=\"r{i}\" href=\"#fn{i}\" role=\"doc-noteref\">1</a>"))
            .collect::<String>();
        let blocks = (0..n)
            .map(|i| format!("<div id=\"fn{i}\">"))
            .collect::<String>();
        let backs = (0..n)
            .map(|i| format!("<a href=\"#r{i}\" class=\"footnote-back\">back</a>"))
            .collect::<String>();
        return format!(
            "<p>{refs}</p><section>{blocks}Note.{backs}{}</section>",
            "</div>".repeat(n)
        );
    }
    if shape == "deep-alias-targets" {
        let refs = (0..n)
            .map(|i| format!("<a href=\"#alias{i}\" role=\"doc-noteref\">1</a>"))
            .collect::<String>();
        let targets = (0..n)
            .map(|i| {
                format!("<span><a id=\"alias{i}\" href=\"#unused\" class=\"footnote-back\">1</a>")
            })
            .collect::<String>();
        return format!(
            "<p>{refs}</p><section><p>{targets}Note.{}</p></section>",
            "</span>".repeat(n)
        );
    }
    if matches!(shape, "long-inverse-class" | "long-inverse-ref-class") {
        let role = if shape == "long-inverse-class" {
            " role=\"doc-noteref\""
        } else {
            ""
        };
        return format!("<p>{}</p><section><p>Note.<a id=\"fn1\" href=\"#r\" class=\"footnote-back {}\"{role}>back</a></p></section>",
            "<a id=\"r\" href=\"#fn1\" role=\"doc-noteref\">1</a>".repeat(n), "noise ".repeat(n));
    }
    let mut refs = String::new();
    let mut notes = String::new();
    for i in 0..n {
        if matches!(shape, "backlinks" | "wrapped-backlinks") {
            refs.push_str(&format!(
                "<a id=\"ref{i}\" href=\"#fn1\" role=\"doc-noteref\">1</a>"
            ));
            let anchor = format!("<a href=\"#ref{i}\" class=\"footnote-back\">back</a>");
            notes.push_str(&if shape == "wrapped-backlinks" {
                format!("<span>{anchor} </span>")
            } else {
                anchor
            });
        } else {
            refs.push_str(&format!("<a href=\"#fn{i}\" role=\"doc-noteref\">1</a>"));
            let paragraph = format!("<p id=\"fn{i}\">Note.</p>");
            notes.push_str(&if shape == "empty-wrappers" {
                format!("<div>{paragraph}</div> \n<!--layout-->")
            } else {
                paragraph
            });
        }
    }
    if matches!(shape, "backlinks" | "wrapped-backlinks") {
        format!("<p>Body {refs}</p><section><p id=\"fn1\">Note.{notes}</p></section>")
    } else {
        format!("<p>{refs}</p><div id=\"endnotes\">{notes}</div><p>Tail.</p>")
    }
}

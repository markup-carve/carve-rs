pub fn source(shape: &str, n: usize) -> String {
    if shape == "note-admonition-titles" {
        let refs: String = (1..=n)
            .map(|i| format!("<a href=\"#fn{i}\" role=\"doc-noteref\">1</a>"))
            .collect();
        let notes: String = (1..=n).map(|i| format!("<div id=\"fn{i}\"><aside class=\"admonition note\" aria-labelledby=\"adm-{i}\"><p class=\"admonition-title\" id=\"adm-{i}\">Title.</p><p>Body.</p></aside></div>")).collect();
        format!("<p>{refs}</p><section>{notes}</section>")
    } else if shape == "admonition-titles" {
        (1..=n).map(|i| format!("<aside class=\"admonition note\" aria-labelledby=\"adm-{i}\"><p class=\"admonition-title\" id=\"adm-{i}\">Title.</p><p>Body.</p></aside>")).collect()
    } else if shape == "blank-table-rows" {
        format!(
            "<table>{}<tr><td>x</td></tr></table>",
            "<tr><td></td></tr>".repeat(n)
        )
    } else if shape == "hard-break-padding" {
        format!("<p>{}</p>", "<br> <em>x</em>".repeat(n))
    } else if shape == "empty-code" {
        format!("<p>{}</p>", "<code></code><span>x</span>".repeat(n))
    } else {
        format!(
            "<p><strong>{}</strong></p>",
            "<strong>x</strong> y ".repeat(n)
        )
    }
}

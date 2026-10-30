use carve::to_html;

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

#[test]
fn tab_opened_run_and_below_column_closer_stay_in_the_description() {
    assert_eq!(
        html(":: t\n: a\n\t```\n ```\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<pre><code></code></pre>"
    );
}

#[test]
fn five_space_run_and_below_column_closer_stay_in_the_description() {
    assert_eq!(
        html(":: m\n: l\n     ```\ne\n ```\n"),
        "<dl>\n  <dt>m</dt>\n  <dd>\n    <p>l</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<p>e</p>\n<pre><code></code></pre>"
    );
}

#[test]
fn a_blank_after_the_below_column_run_ends_the_description() {
    assert_eq!(
        html(":: t\n: a\n\t```\n ```\n\ntext\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<pre><code>\ntext\n</code></pre>"
    );
}

#[test]
fn a_closer_at_the_body_column_still_closes_a_block_fence() {
    assert_eq!(
        html(":: t\n: a\n\t```\n  ```\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>"
    );
}

#[test]
fn the_next_term_still_opens_a_new_entry() {
    assert_eq!(
        html(":: t\n: a\n\t```\n ```\n:: u\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<pre><code>:: u\n</code></pre>"
    );
}

#[test]
fn a_tilde_run_obeys_the_same_column_rule() {
    assert_eq!(
        html(":: t\n: a\n\t~~~\n ~~~\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>a</p>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<pre><code></code></pre>"
    );
}

#[test]
fn formatting_the_strict_controls_keeps_the_description_owner() {
    for source in [":: t\n: a\n\t```\n ```\n", ":: m\n: l\n     ```\ne\n ```\n"] {
        let expected = html(source);
        let formatted = carve::to_carve(source);
        assert_eq!(html(&formatted), expected, "{formatted:?}");
        assert_eq!(carve::to_carve(&formatted), formatted);
    }
}

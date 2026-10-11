use carve::{to_carve, to_html};

#[test]
fn list_closed_text() {
    let source = "- - ```\n    ```\nx\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>";
    assert_eq!(to_html(source).trim_end(), expected);
    let formatted = to_carve(source);
    assert_eq!(to_html(&formatted).trim_end(), expected);
    assert_eq!(to_carve(&formatted), formatted);
}

#[test]
fn list_closed_fence() {
    let source = "- - ```\n    ```\n```\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<pre><code></code></pre>";
    assert_eq!(to_html(source).trim_end(), expected);
    let formatted = to_carve(source);
    assert_eq!(to_html(&formatted).trim_end(), expected);
    assert_eq!(to_carve(&formatted), formatted);
}

#[test]
fn list_unfinished() {
    let source = "- - ```\n    x\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>x\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source).trim_end(), expected);
    let formatted = to_carve(source);
    assert_eq!(to_html(&formatted).trim_end(), expected);
    assert_eq!(to_carve(&formatted), formatted);
}

#[test]
fn description_closed_text() {
    let source = ":: t\n: - - ```\n      ```\nx\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code></code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<p>x</p>";
    assert_eq!(to_html(source).trim_end(), expected);
    let formatted = to_carve(source);
    assert_eq!(to_html(&formatted).trim_end(), expected);
    assert_eq!(to_carve(&formatted), formatted);
}

#[test]
fn description_closed_fence() {
    let source = ":: t\n: - - ```\n      ```\n```\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code></code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<pre><code></code></pre>";
    assert_eq!(to_html(source).trim_end(), expected);
    let formatted = to_carve(source);
    assert_eq!(to_html(&formatted).trim_end(), expected);
    assert_eq!(to_carve(&formatted), formatted);
}

#[test]
fn description_unfinished() {
    let source = ":: t\n: - - ```\n      x\n";
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code>x\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </dd>\n</dl>";
    assert_eq!(to_html(source).trim_end(), expected);
    let formatted = to_carve(source);
    assert_eq!(to_html(&formatted).trim_end(), expected);
    assert_eq!(to_carve(&formatted), formatted);
}

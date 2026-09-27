//! ONE AUTHORED BLOCK BASE closes a fence at two columns and no others
//! (CARVE-P0-004, `resources/spec/01-layout.ebnf`; markup-carve/carve-rs#2068).
//!
//! A fence opened PAST its container's content column takes its own authored
//! column as the local base. A closing run counts there, and at the container's
//! content column where the container reclaims the line. A run strictly between
//! the two is payload, and so is one past the base - the residue differs: below
//! the base a payload line keeps what sits past the CONTAINER column, at or past
//! it what sits past the BASE.
//!
//! MEASURED, NOT ASSUMED. Sixty documents cover five container kinds, two
//! opener overshoots and every closer column from the content column to two past
//! the base, rendered through the executable spec (markup-carve/carve
//! `5315967c`, `scripts/spec/layout.mjs` + `scripts/spec/html.mjs`). Twenty of
//! them read a closer in the band before this guard, and every residue below the
//! base was measured the same way.

use carve::{to_html, to_html_with_options, Options};

/// A parse that answers differently with positions on is a defect of its own.
fn both_paths(src: &str) -> String {
    let html = to_html(src);
    assert_eq!(
        html,
        to_html_with_options(src, &Options::default().with_positions(true)),
        "{src:?}"
    );
    html
}

/// `(source, verbatim content, closer column, where that column sits)`.
type Row = (&'static str, &'static str, usize, &'static str);

fn check(rows: &[Row]) {
    for (src, code, close, band) in rows {
        let html = both_paths(src);
        let verbatim = html
            .split_once("<pre><code>")
            .and_then(|(_, rest)| rest.split_once("</code></pre>"))
            .map(|(verbatim, _)| verbatim)
            .unwrap_or_else(|| panic!("no verbatim block for {src:?}"));
        assert_eq!(verbatim, *code, "closer at column {close}, {band}: {src:?}");
        // A closed fence hands the tail back to the container as prose; an
        // unterminated one swallows it into the payload.
        let outside = html.replace(verbatim, "");
        assert_eq!(
            outside.contains("tail"),
            !code.contains("tail"),
            "closer at column {close}, {band}: {src:?}"
        );
    }
}

/// A list item, content column 2.
#[test]
fn a_fence_in_a_list_item_closes_at_two_columns() {
    check(&[
        (
            "- item\n\n    ```\n    a\n  ```\n\n  tail\n",
            "a\n",
            2,
            "the container column",
        ),
        (
            "- item\n\n    ```\n    a\n   ```\n\n  tail\n",
            "a\n ```\n\ntail\n",
            3,
            "the band",
        ),
        (
            "- item\n\n    ```\n    a\n    ```\n\n  tail\n",
            "a\n",
            4,
            "the authored base",
        ),
        (
            "- item\n\n    ```\n    a\n     ```\n\n  tail\n",
            "a\n ```\n\ntail\n",
            5,
            "past the base",
        ),
        (
            "- item\n\n    ```\n    a\n      ```\n\n  tail\n",
            "a\n  ```\n\ntail\n",
            6,
            "past the base",
        ),
        (
            "- item\n\n      ```\n      a\n  ```\n\n  tail\n",
            "a\n",
            2,
            "the container column",
        ),
        (
            "- item\n\n      ```\n      a\n   ```\n\n  tail\n",
            "a\n ```\n\ntail\n",
            3,
            "the band",
        ),
        (
            "- item\n\n      ```\n      a\n    ```\n\n  tail\n",
            "a\n  ```\n\ntail\n",
            4,
            "the band",
        ),
        (
            "- item\n\n      ```\n      a\n     ```\n\n  tail\n",
            "a\n   ```\n\ntail\n",
            5,
            "the band",
        ),
        (
            "- item\n\n      ```\n      a\n      ```\n\n  tail\n",
            "a\n",
            6,
            "the authored base",
        ),
        (
            "- item\n\n      ```\n      a\n       ```\n\n  tail\n",
            "a\n ```\n\ntail\n",
            7,
            "past the base",
        ),
        (
            "- item\n\n      ```\n      a\n        ```\n\n  tail\n",
            "a\n  ```\n\ntail\n",
            8,
            "past the base",
        ),
    ]);
}

/// A nested list item, content column 4.
#[test]
fn a_fence_in_a_nested_item_closes_at_two_columns() {
    check(&[
        (
            "- a\n  - item\n\n      ```\n      a\n    ```\n\n    tail\n",
            "a\n",
            4,
            "the container column",
        ),
        (
            "- a\n  - item\n\n      ```\n      a\n     ```\n\n    tail\n",
            "a\n ```\n\ntail\n",
            5,
            "the band",
        ),
        (
            "- a\n  - item\n\n      ```\n      a\n      ```\n\n    tail\n",
            "a\n",
            6,
            "the authored base",
        ),
        (
            "- a\n  - item\n\n      ```\n      a\n       ```\n\n    tail\n",
            "a\n ```\n\ntail\n",
            7,
            "past the base",
        ),
        (
            "- a\n  - item\n\n      ```\n      a\n        ```\n\n    tail\n",
            "a\n  ```\n\ntail\n",
            8,
            "past the base",
        ),
        (
            "- a\n  - item\n\n        ```\n        a\n    ```\n\n    tail\n",
            "a\n",
            4,
            "the container column",
        ),
        (
            "- a\n  - item\n\n        ```\n        a\n     ```\n\n    tail\n",
            "a\n ```\n\ntail\n",
            5,
            "the band",
        ),
        (
            "- a\n  - item\n\n        ```\n        a\n      ```\n\n    tail\n",
            "a\n  ```\n\ntail\n",
            6,
            "the band",
        ),
        (
            "- a\n  - item\n\n        ```\n        a\n       ```\n\n    tail\n",
            "a\n   ```\n\ntail\n",
            7,
            "the band",
        ),
        (
            "- a\n  - item\n\n        ```\n        a\n        ```\n\n    tail\n",
            "a\n",
            8,
            "the authored base",
        ),
        (
            "- a\n  - item\n\n        ```\n        a\n         ```\n\n    tail\n",
            "a\n ```\n\ntail\n",
            9,
            "past the base",
        ),
        (
            "- a\n  - item\n\n        ```\n        a\n          ```\n\n    tail\n",
            "a\n  ```\n\ntail\n",
            10,
            "past the base",
        ),
    ]);
}

/// A footnote body, content column 2.
#[test]
fn a_fence_in_a_footnote_body_closes_at_two_columns() {
    check(&[
        (
            "x[^1]\n\n[^1]: note\n\n    ```\n    a\n  ```\n\n  tail\n",
            "a\n",
            2,
            "the container column",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n    ```\n    a\n   ```\n\n  tail\n",
            "a\n ```\n\ntail\n",
            3,
            "the band",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n    ```\n    a\n    ```\n\n  tail\n",
            "a\n",
            4,
            "the authored base",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n    ```\n    a\n     ```\n\n  tail\n",
            "a\n ```\n\ntail\n",
            5,
            "past the base",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n    ```\n    a\n      ```\n\n  tail\n",
            "a\n  ```\n\ntail\n",
            6,
            "past the base",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n  ```\n\n  tail\n",
            "a\n",
            2,
            "the container column",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n   ```\n\n  tail\n",
            "a\n ```\n\ntail\n",
            3,
            "the band",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n    ```\n\n  tail\n",
            "a\n  ```\n\ntail\n",
            4,
            "the band",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n     ```\n\n  tail\n",
            "a\n   ```\n\ntail\n",
            5,
            "the band",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n      ```\n\n  tail\n",
            "a\n",
            6,
            "the authored base",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n       ```\n\n  tail\n",
            "a\n ```\n\ntail\n",
            7,
            "past the base",
        ),
        (
            "x[^1]\n\n[^1]: note\n\n      ```\n      a\n        ```\n\n  tail\n",
            "a\n  ```\n\ntail\n",
            8,
            "past the base",
        ),
    ]);
}

/// A definition description, content column 3.
#[test]
fn a_fence_in_a_definition_description_closes_at_two_columns() {
    check(&[
        (
            ":: t\n:  d\n\n     ```\n     a\n   ```\n\n   tail\n",
            "a\n",
            3,
            "the container column",
        ),
        (
            ":: t\n:  d\n\n     ```\n     a\n    ```\n\n   tail\n",
            "a\n ```\n\ntail\n",
            4,
            "the band",
        ),
        (
            ":: t\n:  d\n\n     ```\n     a\n     ```\n\n   tail\n",
            "a\n",
            5,
            "the authored base",
        ),
        (
            ":: t\n:  d\n\n     ```\n     a\n      ```\n\n   tail\n",
            "a\n ```\n\ntail\n",
            6,
            "past the base",
        ),
        (
            ":: t\n:  d\n\n     ```\n     a\n       ```\n\n   tail\n",
            "a\n  ```\n\ntail\n",
            7,
            "past the base",
        ),
        (
            ":: t\n:  d\n\n       ```\n       a\n   ```\n\n   tail\n",
            "a\n",
            3,
            "the container column",
        ),
        (
            ":: t\n:  d\n\n       ```\n       a\n    ```\n\n   tail\n",
            "a\n ```\n\ntail\n",
            4,
            "the band",
        ),
        (
            ":: t\n:  d\n\n       ```\n       a\n     ```\n\n   tail\n",
            "a\n  ```\n\ntail\n",
            5,
            "the band",
        ),
        (
            ":: t\n:  d\n\n       ```\n       a\n      ```\n\n   tail\n",
            "a\n   ```\n\ntail\n",
            6,
            "the band",
        ),
        (
            ":: t\n:  d\n\n       ```\n       a\n       ```\n\n   tail\n",
            "a\n",
            7,
            "the authored base",
        ),
        (
            ":: t\n:  d\n\n       ```\n       a\n        ```\n\n   tail\n",
            "a\n ```\n\ntail\n",
            8,
            "past the base",
        ),
        (
            ":: t\n:  d\n\n       ```\n       a\n         ```\n\n   tail\n",
            "a\n  ```\n\ntail\n",
            9,
            "past the base",
        ),
    ]);
}

/// A list item inside a block quote, content column 2.
#[test]
fn a_fence_in_a_quoted_list_item_closes_at_two_columns() {
    check(&[
        (
            "> - item\n>\n>     ```\n>     a\n>   ```\n>   \n>   tail\n",
            "a\n",
            2,
            "the container column",
        ),
        (
            "> - item\n>\n>     ```\n>     a\n>    ```\n>   \n>   tail\n",
            "a\n ```\n\ntail\n",
            3,
            "the band",
        ),
        (
            "> - item\n>\n>     ```\n>     a\n>     ```\n>   \n>   tail\n",
            "a\n",
            4,
            "the authored base",
        ),
        (
            "> - item\n>\n>     ```\n>     a\n>      ```\n>   \n>   tail\n",
            "a\n ```\n\ntail\n",
            5,
            "past the base",
        ),
        (
            "> - item\n>\n>     ```\n>     a\n>       ```\n>   \n>   tail\n",
            "a\n  ```\n\ntail\n",
            6,
            "past the base",
        ),
        (
            "> - item\n>\n>       ```\n>       a\n>   ```\n>   \n>   tail\n",
            "a\n",
            2,
            "the container column",
        ),
        (
            "> - item\n>\n>       ```\n>       a\n>    ```\n>   \n>   tail\n",
            "a\n ```\n\ntail\n",
            3,
            "the band",
        ),
        (
            "> - item\n>\n>       ```\n>       a\n>     ```\n>   \n>   tail\n",
            "a\n  ```\n\ntail\n",
            4,
            "the band",
        ),
        (
            "> - item\n>\n>       ```\n>       a\n>      ```\n>   \n>   tail\n",
            "a\n   ```\n\ntail\n",
            5,
            "the band",
        ),
        (
            "> - item\n>\n>       ```\n>       a\n>       ```\n>   \n>   tail\n",
            "a\n",
            6,
            "the authored base",
        ),
        (
            "> - item\n>\n>       ```\n>       a\n>        ```\n>   \n>   tail\n",
            "a\n ```\n\ntail\n",
            7,
            "past the base",
        ),
        (
            "> - item\n>\n>       ```\n>       a\n>         ```\n>   \n>   tail\n",
            "a\n  ```\n\ntail\n",
            8,
            "past the base",
        ),
    ]);
}

/// The base dedent is ALL OR NOTHING. A payload line that does not reach the
/// base keeps every column past the container's; one that reaches it keeps only
/// what sits past the base. Content column 2, base 8.
#[test]
fn a_payload_line_below_the_base_keeps_its_container_residue() {
    let html = both_paths("- item\n\n        ```\n  a\n   b\n    c\n     d\n      e\n       f\n        g\n         h\n          i\n        ```\n");
    let verbatim = html
        .split_once("<pre><code>")
        .and_then(|(_, rest)| rest.split_once("</code></pre>"))
        .map(|(verbatim, _)| verbatim)
        .expect("verbatim block");
    assert_eq!(verbatim, "a\n b\n  c\n   d\n    e\n     f\ng\n h\n  i\n");
}

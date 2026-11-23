// carve#6440: a fence opened behind a task box, a footnote label or glued attributes strips up to its opener's indent
// from each line (C2-B-029), so a value line that starts with whitespace is written at the opener's column, and the
// closer with it (C2-W-033).
use carve::api::{Carve, Options, WriteOptions};

#[test]
fn fences_behind_a_lead_keep_their_indent() {
    let cases = [
        "- [x] ```\n        y\n      ```\n",
        "- [ ] ```js\n        y\n\n      z\n      ```\n",
        ". [x] ```\n          y\n      ```\n",
        "-{.a} ```\n        y\n      ```\n",
        "[^n]: ```\n        y\n      ```\n",
        "- - [x] ```\n          y\n        ```\n",
        "> - [x] ```\n>         y\n>       ```\n",
        "[^n]: > - [x] ```\n  >         y\n  >       ```\n",
        "- [x] ```=html\n        <b>\n      ```\n",
        "- [x] %%%\n        y\n      %%%\n",
        // Without whitespace at the start of a line, the lines stand at the content column (C2-W-024).
        "- [x] ```\n  y\n  ```\n",
        // The same fence without the task box.
        "- ```\n      y\n  ```\n",
    ];
    let carve = Carve::new();
    for input in cases {
        for normalize in [false, true] {
            let options = WriteOptions {
                normalize,
                ..WriteOptions::default()
            };
            assert_eq!(carve.format(input, &options).unwrap(), input, "{input:?}");
            let tree = carve.parse(input, &Options::default()).unwrap();
            assert_eq!(carve.write(&tree, &options).unwrap(), input, "{input:?}");
        }
    }
}

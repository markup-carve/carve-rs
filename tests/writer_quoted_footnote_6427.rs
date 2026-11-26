use carve::api::{Carve, Options, WriteOptions};

#[test]
fn quoted_footnote_items_count_columns_after_the_quote() {
    let cases = [
        "[^n]: > - ```\n  >   y\n  >   ```\n",
        "[^n]: > - a\n  >\n  >   b\n",
        "[^n]: > - - ```\n  >     y\n  >     ```\n",
        "[^n]: > > - ```\n  > >   y\n  > >   ```\n",
        "[^n]: > . ```\n  >   y\n  >   ```\n",
        "[^n]: > - > ```\n  >   > y\n  >   > ```\n",
        "[^n]: > - [x] a\n  >\n  >   b\n",
        "[^n]: - a\n\n        b\n",
        "[^n]: - > - a\n        >\n        >   b\n",
    ];
    let carve = Carve::new();
    for input in cases {
        for normalize in [false, true] {
            let options = WriteOptions {
                normalize,
                ..WriteOptions::default()
            };
            assert_eq!(carve.format(input, &options).unwrap(), input);
            let tree = carve.parse(input, &Options::default()).unwrap();
            assert_eq!(carve.write(&tree, &options).unwrap(), input);
        }
    }
}

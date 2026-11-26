use carve::api::{Carve, WriteOptions};

#[test]
fn rebuilt_column_keys_precede_rebuilt_row_keys_after_retained_keys() {
    let input = "{table header-rows=2 z=2 widths=10,20 aligns=left,right a=1 footer-rows=1}\n```csv\nh,i\ng,j\nx,y\nf,g\n```\n";
    let expected = "{table z=2 a=1 aligns=left,right widths=10,20 header-rows=2 footer-rows=1}\n```csv\nh,i\ng,j\nx,y\nf,g\n```\n";
    assert_eq!(
        Carve::new()
            .format(input, &WriteOptions::default())
            .unwrap(),
        expected
    );
}

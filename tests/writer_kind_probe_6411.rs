use carve::api::{Carve, Options, WriteOptions};

fn nested_links(n: usize) -> String {
    format!("{}^*[*x](u){}\n", "^*[*[[*".repeat(n), "^]]](u)".repeat(n))
}

// carve#6411: the probe rounds escape by kind and partner bracket (C2-W-088,
// C2-W-125). The expected bytes are carve-js next2's.
#[test]
fn nested_links_write_the_reference_bytes() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for (n, expected) in [
        (50, include_str!("data/writer-6411/n50.out")),
        (82, include_str!("data/writer-6411/n82.out")),
    ] {
        let source = nested_links(n);
        let written = carve.format(&source, &options).unwrap();
        assert_eq!(written, expected, "n = {n}");
        let tree = carve.parse(&source, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &options).unwrap(), expected, "n = {n}");
    }
}

// CC-1568: a closer that ends a probed node early gives no kind, so bare carets stay bare.
#[test]
fn closer_repairs_give_no_kind_in_a_probe() {
    let source = "^{^{^{^{^^}\\{^^{^^}^{^^}{^^^^{^^}\\{^{^^^}^}^}^}^}^}\n";
    let written = Carve::new()
        .format(source, &WriteOptions::default())
        .unwrap();
    assert_eq!(
        written,
        "^{^{^{^\\{^\\^}\\{^^\\{^\\^}^\\{^\\^}{^\\^^^\\{^\\^}\\{^{^^^}^}^}^}^}^}\n"
    );
}

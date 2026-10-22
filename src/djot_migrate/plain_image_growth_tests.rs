#[test]
fn reference_destinations_remain_opaque() {
    let source = "[t][r]\n\n[r]:\n  ![a\n  ![b](x)](y)";
    assert_eq!(
        super::collapse_plain_nested_image_chains(source).as_ref(),
        source
    );
}

use carve::{select_include_region, IncludeRegion, IncludeRegionMode};

#[test]
fn region_selection_follows_mode_and_physical_lines() {
    let source =
        "// #region code\rA\r// #endregion code\r%% #region carve\rB\r%% #endregion carve\r";
    assert_eq!(
        select_include_region(source, "code", IncludeRegionMode::Code),
        Ok(IncludeRegion { start: 2, end: 2 })
    );
    assert_eq!(
        select_include_region(source, "carve", IncludeRegionMode::Carve),
        Ok(IncludeRegion { start: 5, end: 5 })
    );
    assert_eq!(
        select_include_region(source, "code", IncludeRegionMode::Carve)
            .unwrap_err()
            .rule(),
        "include-region-invalid"
    );
    assert_eq!(
        select_include_region(
            "%% #region empty\r%% #endregion\r",
            "empty",
            IncludeRegionMode::Carve
        ),
        Ok(IncludeRegion { start: 2, end: 1 })
    );
}

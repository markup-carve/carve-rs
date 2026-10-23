//! Compile-time coverage for the root and module paths kept for existing callers.

#[test]
fn common_root_and_module_paths_remain_available() {
    let _ = carve::parse;
    let _ = carve::to_html;
    let _ = carve::from_json;
    let _ = carve::try_to_json;
    let _ = carve::render_carve;
    let _ = carve::djot_to_carve;
    let _ = carve::markdown_to_carve;
    let _ = carve::ast_json::from_json;
    let _ = carve::ast_json::try_to_json;
    let _ = carve::djot_migrate::djot_to_carve;
    let _ = carve::markdown_import::markdown_to_carve;
    let _ = std::mem::size_of::<carve::Document>();
    let _ = std::mem::size_of::<carve::ast::Document>();
    let _ = std::mem::size_of::<carve::BlockNode>();
    let _ = std::mem::size_of::<carve::ast::BlockNode>();
    let _ = std::mem::size_of::<carve::Options<'static>>();
    let _ = std::mem::size_of::<carve::extensions::fenced_render::FencedRender>();
}

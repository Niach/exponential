//! #FEED-244: a REAL headless render of an app screen. Skips (passes, with
//! a note) on a machine without a headless GPU adapter, unless
//! `EXP_REQUIRE_GPU` says it has one (#EXP-1825's rule).

use serde_json::json;

#[test]
fn an_app_screen_renders_to_a_png_of_its_size() {
    if let Err(e) = gpui_platform::current_headless_renderer() {
        // #EXP-1825: a runner that has a GPU (EXP_REQUIRE_GPU) fails instead.
        assert!(std::env::var("EXP_REQUIRE_GPU").as_deref() != Ok("1"), "no headless GPU adapter ({e:#})");
        eprintln!("skipped: no headless GPU adapter ({e:#})");
        return;
    }
    let bundle = eui_shot::parse_bundle(&json!({
        "schema": 1,
        "package": {
            "id": "demo",
            "catalogId": "https://ui.exponential.at/catalogs/core/v1",
            "templates": {"home": {"components": [
                {"id": "root", "component": "Stack", "children": ["h", "t", "b"]},
                {"id": "h", "component": "Heading", "text": {"@path": "/title"}},
                {"id": "t", "component": "Text", "text": "Resolved on the server"},
                {"id": "b", "component": "Button", "label": "Save"}
            ]}}
        },
        "templateId": "home",
        "data": {"title": "Groceries"},
        "width": 390,
        "height": 844
    }))
    .unwrap();
    let image = eui_shot::render(&bundle).expect("a picture");
    // Scale factor 1 headless: the bundle's size, in pixels (or ×2 on a
    // retina-scaled renderer).
    assert_eq!(image.width() * 844, image.height() * 390, "{}x{}", image.width(), image.height());
    // Not one flat colour: the heading, the text and the button painted.
    let first = *image.get_pixel(0, 0);
    assert!(image.pixels().any(|p| *p != first), "a blank frame");
    let png = eui_shot::png(&image).unwrap();
    assert_eq!(&png[1..4], b"PNG");
}

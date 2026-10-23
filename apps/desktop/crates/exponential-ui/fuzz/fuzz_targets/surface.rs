//! A live surface fed arbitrary server messages (or one array of them):
//! `apply` answers Ok or Err, the surface stays consistent, and a fixed
//! layout of whatever landed never panics.
#![no_main]

use exponential_ui::surface::{Surface, SurfaceOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else { return };
    let mut surface = Surface::new("fuzz", SurfaceOptions::default());
    let messages: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    };
    for m in messages.iter().take(32) {
        let _ = surface.apply(m);
    }
    surface.set_viewport(320.0, 480.0, None);
    let _ = surface.issues();
    let _ = surface.data();
});

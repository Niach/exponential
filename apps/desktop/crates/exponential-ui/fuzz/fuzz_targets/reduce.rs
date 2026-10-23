//! Reduce + expand on arbitrary JSON: a flat component list or a nested
//! tree, through the core catalog and the basic catalog. Never a panic.
#![no_main]

use exponential_ui::catalog::{A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID};
use exponential_ui::reducer::{reduce_nested, reduce_surface, ReduceOptions};
use exponential_ui::types::{FlatComponent, NestedNode};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else { return };
    for catalog in [CORE_CATALOG_ID, A2UI_BASIC_CATALOG_ID] {
        let options = ReduceOptions::new(catalog);
        if let Ok(flat) = serde_json::from_value::<Vec<FlatComponent>>(value.clone()) {
            let out = reduce_surface(&flat, &options);
            let _ = serde_json::to_string(&out);
        }
        if let Ok(nested) = serde_json::from_value::<NestedNode>(value.clone()) {
            let out = reduce_nested(&nested, &options);
            let _ = serde_json::to_string(&out);
        }
    }
});

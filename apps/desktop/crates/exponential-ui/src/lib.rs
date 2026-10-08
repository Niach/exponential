//! # Exponential UI core (VAPP-86)
//!
//! The engine every native Exponential UI painter (gpui, SwiftUI, Compose)
//! runs on: an [A2UI](https://a2ui.org) surface comes in as messages, the
//! **reducer** turns the flat component list into ONE normalized tree in the
//! core vocabulary (the A2UI basic catalog mapped onto it, macros expanded
//! through the generated table, extension catalogs validated and their
//! natives passed through as `Extension` nodes), the **theme** resolves every
//! `$token` reference and per-part recipe into concrete visuals, **taffy**
//! lays the tree out against the surface width with a batched two-phase text
//! measurement that crosses the FFI at most three times per pass, **overlay
//! layers** (Dialog, Drawer, Popover, Tooltip, DropdownMenu) get their own
//! roots placed against the viewport or an anchor frame, and windowed `List`s
//! lay out only the visible rows. The painter receives flat `PlacedNode`s with
//! RESOLVED visuals and paints; it never reads the catalog or the theme.
//!
//! The TypeScript reference implementation lives in `packages/exponential-ui`
//! (`@exponential-at/ui`); both replay the same fixtures byte for byte, and
//! the catalog, macro table and built-in themes are embedded here from the
//! same generator (`src/generated`).
//!
//! The coarse mobile facade is the sibling crate `exponential-ui-ffi`.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod generated;

pub mod basic_map;
pub mod bench;
pub mod catalog;
pub mod data;
pub mod expr;
pub mod extension;
pub mod geometry;
pub mod json;
pub mod layout_tree;
pub mod list;
pub mod macros;
pub mod measure;
pub mod overlay;
pub mod recipes;
pub mod reducer;
pub mod style;
pub mod style_check;
pub mod surface;
pub mod theme;
pub mod themes;
pub mod tracks;
pub mod types;
pub mod validate;

#[cfg(feature = "shaping")]
pub mod shaping;

pub use catalog::{
    is_known_token, parse_token_ref, CatalogView, A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID,
    CORE_LITE_CATALOG_ID, UNKNOWN_COMPONENT,
};
pub use taffy;
pub use types::*;

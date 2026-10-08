//! # Exponential UI gpui painter (VAPP-90)
//!
//! The desktop painter of the Exponential UI SDK. It uses the core
//! (`exponential-ui`) DIRECTLY — no FFI: the core reduces the A2UI surface,
//! resolves the theme and lays the tree out with taffy against a gpui
//! text-system `Measure`; this crate paints every placed node as an absolute
//! gpui element at the core's frame (nested like the tree so clipping and
//! opacity inherit), never mapping the catalog onto gpui's `Styled` layout.
//!
//! Modules:
//! - [`host`]: the [`host::HostPlugin`] an embedding app provides (icons,
//!   actions, input echoes, URLs, markdown, unknown components).
//! - [`measure`]: the in-process `Measure` over gpui's text system.
//! - [`view`]: [`view::SurfaceView`], the gpui entity that owns a surface.
//! - [`paint`]: the native painters, one per catalog kind + the overlays and
//!   the windowed list.
//! - [`extension`]: the painter trait registered per extension kind.
//! - [`chrome`] + [`controls`]: the generic glass controls the IDE and the SDK
//!   share, keyed on a host-installed [`chrome::Chrome`].

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod chrome;
pub mod controls;
pub mod extension;
pub mod host;
pub mod measure;
pub mod paint;
pub mod view;

pub use exponential_ui;

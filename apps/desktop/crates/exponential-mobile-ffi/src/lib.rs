//! #PERF-11: the phone apps' single native library.
//!
//! Each facade keeps its own UniFFI namespace (`exponential_ui_ffi`,
//! `eengine_ffi`, `exponential_net_ffi`, `chat_store_ffi`): every scaffolding symbol carries it,
//! so the three link side by side and their committed bindings load them from
//! here unchanged. Kotlin: `MobileNativeLibrary` points each binding at
//! `libexponential_mobile_ffi.so` (`uniffi.component.<namespace>.libraryOverride`);
//! Swift: the symbols resolve at link time. The `pub use`s are what link the
//! crates (and with them their `#[no_mangle]` exports) into this library.

pub use chat_store_ffi;
pub use eengine_ffi;
pub use exponential_net_ffi;
pub use exponential_ui_ffi;

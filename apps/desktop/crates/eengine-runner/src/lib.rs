//! E6 (#VAPP-266): the desktop runner, the prebuilt shell an exported app
//! runs in on macOS, Windows and Linux. The contract is
//! `packages/eengine/fixtures/runner.json` (its `$comment` is the spec).
//!
//! The lib is the painter-free glue, tested headless: where the bundle sits
//! ([`bundle`]), the app-data paths ([`storage`]), the Vault in the
//! platform keystore ([`vault`]), what reaches the runner from outside
//! ([`events`]), the one Engine instance with what each event means for
//! the window ([`runner`]) and the one running instance per app id on
//! Linux and Windows ([`single`]). `main.rs` paints it with eui-gpui.
//!
//! No chrome of its own, no updater, no telemetry, no account: every pixel
//! is a genUI surface (`app`, `consent`, `permissions`, `toast`), and
//! nothing in this crate installs a log, crash or analytics sink.

pub mod assets;
pub mod bundle;
pub mod events;
pub mod runner;
pub mod single;
pub mod storage;
pub mod vault;

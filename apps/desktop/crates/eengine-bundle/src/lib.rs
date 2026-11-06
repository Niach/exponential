//! E7 (#VAPP-270, epic #VAPP-259): `exponential bundle`. A signed package, one
//! SVG icon and bundle.toml become native binaries on the prebuilt runners
//! (runner.json `bundle`, docs/spec/apps.md §13.3): nothing is compiled per
//! app except iOS. Locked by `packages/eengine/fixtures/bundle.json` (its
//! `$comment` is the spec). No updater, no update feed, no telemetry, no
//! account: a new package version is a rebundle.

pub mod contract;
pub mod icons;
pub mod kit;
pub mod manifest;
pub mod prepare;
pub mod run;
pub mod targets;

pub use prepare::{prepare, Prepared, Refusal};
pub use run::{run, Options};

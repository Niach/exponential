//! The generated constants, COPIED here by `packages/exponential-ui/scripts/generate.ts`
//! (`bun run --filter @exponential-at/ui generate`) so the crate packages
//! standalone; `tests/generated_drift.rs` fails when the copies and the
//! package's `generated/*.rs` differ. Never edit by hand.

#[allow(dead_code)]
pub mod catalog {
    include!("catalog.rs");
}

#[allow(dead_code)]
pub mod themes {
    include!("themes.rs");
}

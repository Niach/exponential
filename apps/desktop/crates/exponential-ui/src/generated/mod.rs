//! The generated constants, COPIED here by `packages/exponential-ui/scripts/generate.ts`
//! (`bun run --filter @exponential_app/ui generate`) so the crate packages
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

/// VAPP-106: the catalog's descriptions and examples, only with the
/// `descriptions` feature (`crate::catalog::descriptions`).
#[cfg(feature = "descriptions")]
#[allow(dead_code)]
pub mod descriptions {
    include!("descriptions.rs");
}

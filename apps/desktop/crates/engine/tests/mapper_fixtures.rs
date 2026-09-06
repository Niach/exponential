//! EXP-746 — recorded agent frames → `Vec<ActivityEvent>`, compared as parsed
//! `serde_json::Value`s.
//!
//! Owned by lane E1 (fixtures under `tests/fixtures/{claude,codex,pi}/` come
//! from E2/E3/E4). NEVER compare serialized JSON as a STRING: `serde_json`'s
//! `preserve_order` feature is unified ON across this workspace, so key order
//! flips between a single-crate run and a multi-crate one and a string
//! assertion passes alone and flakes under `cargo test`.

#[test]
fn the_mapper_fixture_harness_is_wired_up() {
    // EXP-746 E1: fill — load each fixture, run it through `Mapper`, compare
    // the published events as parsed Values.
    let recorded = engine::RecordingSink::new();
    assert!(recorded.snapshot().is_empty());
}

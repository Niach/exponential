//! The eMCP contract (`packages/emcp`, #EXP-1490), pure: each module
//! mirrors one `fixtures/*.json` and `tests/fixtures.rs` replays every case,
//! like the TypeScript references and the server broker do.

pub mod access;
pub mod actions;
pub mod audit;
pub mod exposes;
pub mod upstream;
pub mod vault;

/// The fixtures, compiled in (a git checkout of the tag carries them).
pub mod fixture {
    use std::sync::OnceLock;

    use serde_json::Value;

    macro_rules! fixture {
        ($name:ident, $path:literal) => {
            pub fn $name() -> &'static Value {
                static CELL: OnceLock<Value> = OnceLock::new();
                CELL.get_or_init(|| serde_json::from_str(include_str!($path)).expect(concat!($path, " is JSON")))
            }
        };
    }

    fixture!(exposes, "../../../../../../packages/emcp/fixtures/exposes.json");
    fixture!(access, "../../../../../../packages/emcp/fixtures/access.json");
    fixture!(audit, "../../../../../../packages/emcp/fixtures/audit.json");
    fixture!(vault, "../../../../../../packages/emcp/fixtures/vault.json");
    fixture!(upstream, "../../../../../../packages/emcp/fixtures/upstream.json");
    fixture!(app_actions, "../../../../../../packages/domain-contract/fixtures/app-actions.json");
}

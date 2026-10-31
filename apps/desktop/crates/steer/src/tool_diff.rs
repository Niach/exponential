//! EXP-1426: the module lives in `eagent::wire::tool_diff` now
//! and is re-exported here whole (its inlined caps are pinned to the contract
//! in `frames::contract_tests`).

pub use eagent::wire::tool_diff::*;

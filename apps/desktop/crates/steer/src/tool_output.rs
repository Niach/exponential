//! EXP-1426: the module lives in `exponential_agent_runtime::wire::tool_output` now
//! and is re-exported here whole (its inlined caps are pinned to the contract
//! in `frames::contract_tests`).

pub use exponential_agent_runtime::wire::tool_output::*;

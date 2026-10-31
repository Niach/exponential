//! Deprecated (#EXP-1505): the agent runtime is now `eagent`. This shim
//! re-exports it for one release so `exponential_agent_runtime::…` paths keep
//! building; depend on `eagent` instead (docs/libraries.md, "Migrating").

pub use eagent::*;

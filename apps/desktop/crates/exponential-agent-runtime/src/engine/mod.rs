//! The ACP engine core (EXP-1426 part 4, moving out of Exponential's
//! `engine` crate, which re-exports every module here under its old path).
//!
//! So far: the child-process line transport and the child's exit link, the
//! event seam ([`sink::EventSink`]), the local feed vocabulary, the pure
//! policies the run ticker drives (stall watchdog, API-error retry, the
//! compaction ask), and (part 4b) the ONE `SessionUpdate` → wire mapper with
//! its reply cards and the [`host_tools::HostTools`] seam, the
//! `context_layout` arithmetic, the result refs and the ACP `terminal/*`
//! capability. The ACP host, the session and the adapters follow behind
//! `RunBackend` (see the README).

pub mod api_retry;
pub mod child_exit;
pub mod compaction;
pub mod context_layout;
pub mod exp_tool_refs;
pub mod host_tools;
pub mod local;
pub mod mapper;
pub mod reply;
pub mod sink;
pub mod stall;
pub mod terminals;
pub mod transport;

pub use child_exit::ChildExitLink;
pub use host_tools::{HostTools, NoHostTools, ScriptCall, ScriptReport};
pub use mapper::{Mapper, MapperConfig};
pub use reply::{ReplyCard, ReplyOption};
pub use sink::{EventSink, RecordingSink};
pub use transport::{spawn_lines, ChildLines, LineWriter, StderrPolicy};

//! The ACP engine core (EXP-1426 part 4, moving out of Exponential's
//! `engine` crate, which re-exports every module here under its old path).
//!
//! So far: the child-process line transport and the child's exit link, the
//! event seam ([`sink::EventSink`]), the local feed vocabulary, and the pure
//! policies the run ticker drives (stall watchdog, API-error retry, the
//! compaction ask). The ACP host, the mapper, the session and the adapters
//! follow behind `RunBackend` (see the README).

pub mod api_retry;
pub mod child_exit;
pub mod compaction;
pub mod local;
pub mod sink;
pub mod stall;
pub mod transport;

pub use child_exit::ChildExitLink;
pub use sink::{EventSink, RecordingSink};
pub use transport::{spawn_lines, ChildLines, LineWriter, StderrPolicy};

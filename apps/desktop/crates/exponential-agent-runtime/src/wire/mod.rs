//! The event vocabulary (EXP-1426 part 2, moved out of `steer`): the relay
//! wire's frames, [`ActivityEvent`] and its pieces, the workflow payload, the
//! redactor and the relay caps. Pure serde data and pure functions; the relay
//! client, publisher and control channel stay in `steer`, which re-exports
//! every module here under its old path (`steer::frames`, `steer::activity`,
//! `steer::workflow`, `steer::tool_output`, `steer::tool_diff`), so the serde
//! wire is byte-identical and no caller changed.
//!
//! Contract vocabularies (`domain::contract`) stay checked in `steer`'s
//! tests: this crate depends on no app crate, so the few caps it needs are
//! inlined and pinned there.

pub mod activity;
pub mod frames;
pub mod tool_diff;
pub mod tool_output;
pub mod workflow;

pub use activity::{
    clamp_config_state, normalize_agent_title, normalize_compaction_trigger,
    now_unix_millis, secrets_from_worktree, synthetic_question_id, truncate, truncate_marked, worktree_diff,
    Redactor, RemoteAnswer, SessionAgent, SessionBlocked, TRUNCATION_MARKER,
};
pub use frames::{
    ActivityEvent, BackgroundTask, BackgroundTaskKind, ConfigCommand, ConfigMode, ConfigOption,
    ConfigValue, QueuedMessage, QuestionOption, ToolKind, TurnState,
};
pub use workflow::{WorkflowAgent, WorkflowAgentState, WorkflowPhase, WorkflowState, WorkflowStatus};

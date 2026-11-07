//! The Engine (#VAPP-259): one runtime for Apps in every host. The Apps
//! format and its pure logic are genUI's host (`eui_core::host`); the Engine
//! is the stateful, I/O-bearing part every host re-implemented: app instances
//! and their lifecycle, sources over a [`providers::RecordStore`], dispatch,
//! the outbox, consent and the capability broker, over the providers a host
//! hands it ([`providers::HostConfig`]).
//!
//! The contract is `packages/eengine` (fixtures + the TS twin
//! `@exp/eengine`); `tests/fixtures.rs` replays it. Modules never depend on
//! the Engine; the Engine takes them through their release tags, the heavy
//! ones behind features (`sqlite`, `hub`).

pub mod broker;
pub mod capability;
pub mod consent;
pub mod contract;
#[cfg(feature = "hub")]
pub mod hub;
pub mod instance;
pub mod lifecycle;
pub mod outbox;
pub mod package;
pub mod pdf;
pub mod providers;
pub mod runner;
#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod stores;
pub mod synced;
pub mod tools;

pub use broker::{broker_audit_row, broker_decide, BrokerCall, CallFrom, GrantValue, Grants, ManifestEntry, Verdict, VerdictKind};
pub use lifecycle::{lifecycle_step, Effect, LifecycleContext, LifecycleEvent, LifecycleState, Phase};
pub use providers::{
    BlobStore, Clock, EngineIdentity, Failure, HostConfig, HostInfo, HostZone, Identity, NetworkPolicy, Presentation, Pushed, RecordStore, RecordWrite, Subscription, SyncTransport, Vault, WindowHost, WriteAck, WriteBatch,
};
pub use capability::{Broker, BrokerAnswer, CallAudit, Capability, ConsentSheet, ConsentUi};
pub use runner::{runner_config, runner_failure, runner_package, runner_toast, FormFactor, RunnerConfig, RunnerFailure};
pub use instance::{brokered, AppInstance, CallResult, LoadError};
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteRecordStore;
pub use stores::{FsBlobStore, LocalIdentity, MemoryRecordStore, MemoryVault};
pub use synced::{record_call, OutboxLog, Submitted, SyncedListener, SyncedRecordStore};

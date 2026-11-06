//! elocal (#EDE-55): the shared LOCAL STORE of a Local desktop.
//!
//! A Local desktop is one team that lives on this machine: no Exponential
//! account, no server, no sync. Its store is the IDE's synced-shape SQLite
//! ([`sync::store::ShapeStore`]: one TEXT-column table per shape, the same
//! 34 tables every client holds) made AUTHORITATIVE: the [`LocalExecutor`]
//! answers the tRPC procedures the clients call over it, behind
//! [`api::Transport`], so the typed wrappers in `api` (and every screen above
//! them) never know whether they talk to an instance or to this file. The
//! rules it replays are `packages/domain-contract/fixtures/local-parity.json`
//! ([`parity`], the TS reference's twin); a procedure the fixture does not
//! list is refused with the fixture's words (HTTP 501 on the seam).
//!
//! What else lives here: the [`Desktops`] index (`desktops.json`: the Local
//! desktops of a machine, the current one, the archived ones after a Share),
//! the [`LocalIdentity`] (the person at the machine, minted once per
//! desktop), the [`FsBlobStore`] (attachment bytes, served as
//! `/api/attachments/{id}` over the seam) and the desktop's sidecar
//! ([`DesktopMeta`]: what the shapes leave out, prompt bodies and agent
//! instructions). Hosts drain [`LocalStore::deltas`] to redraw.
//!
//! gpui-free and Engine-free (`tests/dependency_gate.rs`): eDE hosts it now,
//! the IDE after the parity gate, the Engine's record store later.

pub mod blobs;
pub mod desktops;
pub mod executor;
pub mod identity;
pub mod parity;
pub mod store;
pub mod time;

pub use blobs::FsBlobStore;
pub use desktops::{DesktopEntry, DesktopMeta, Desktops, LocalDesktop, SharedAs};
pub use executor::LocalExecutor;
pub use identity::LocalIdentity;
pub use store::{LocalDelta, LocalStore};

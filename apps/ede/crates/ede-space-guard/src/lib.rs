//! Exponential Desktop's space guard (#EDE-146, #EDE-147, #EDE-148).
//!
//! An agent space's programs run as the person's user (#EDE-145), so the
//! person's microphone, camera and session are theirs too by uid. What
//! tells them apart is the systemd slice every process of a space runs in
//! (#EDE-138). The guard is the one privileged part: a system service
//! (`exponential-space-guard.socket`) with three capabilities and no root
//! that, when a process INSIDE a space slice asks, attaches fixed eBPF
//! programs to that slice. The person's uid can neither load, detach nor
//! replace them. What they refuse: [`devices::RULES`].
//!
//! A hostile caller can only make the guard restrict a cgroup of its own
//! user's: it writes no file, touches no other user, detaches nothing.

pub mod cgroup;
pub mod client;
pub mod devices;
pub mod ebpf;
pub mod guard;
pub mod protocol;
pub mod sys;

pub use protocol::{Request, SOCKET};

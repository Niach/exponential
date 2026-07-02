//! `api` — tRPC-over-HTTP mutations + auth (masterplan-v3 §3.1).
//!
//! Phase 2/3 lands: the tRPC mutation client, the `awaitTxId` sync gate
//! (mirrors the web `generateTxId` handshake), the Better Auth session
//! lifecycle, the auto-minted hidden `expu_` personal key (EXP-2a), and
//! `login.rs` / `token_store.rs` (keyring with a 0600 file fallback) / the
//! `opener` chain for OAuth. Consumed by `ui` for mutations and by
//! `coding`/`steer` for tokens.

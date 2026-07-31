//! The pi observer sidecar (EXP-383): one loopback HTTP server per process
//! that receives batched extension events from `.exp-pi-observer.ts` (POST
//! `/events`) and hands remote steer text back over a long-poll (POST
//! `/steer`). The `-e`-loaded extension is the only client; a bearer token
//! minted at startup gates both routes.
//!
//! Placeholder module — filled in by the pi milestone.

# Vendored iroh 1.4.1 (Exponential patch, #EXP-1484)

Source: the `iroh` 1.4.1 crate as published on crates.io
(checksum `e3689cab508af9bcef7816c1201b403595ec6aa20e7d15e1f9683bb4be4a5b17`),
by n0, inc. and the iroh contributors, licensed MIT OR Apache-2.0
(https://github.com/n0-computer/iroh). `LICENSE-BSD3` is the crate's own
notice for the Tailscale-derived code and is kept unchanged.

Consumed through `[patch.crates-io]` in `apps/desktop/Cargo.toml`. Remove the
patch and this directory once an iroh release carries the fix.

## Changes (Apache-2.0 section 4(b))

- `src/socket/remote_map/remote_state.rs`, `handle_connection_close`: when a
  closed connection was the only one carrying the selected path while other
  connections to the same remote remain, clear `selected_path` and call
  `select_path()`. Upstream keeps the dead path selected, so the next
  handshake to a peer that restarted on a new address and redialed goes to the
  dead socket until the handshake times out.
- Packaging only: dropped `examples/`, `tests/`, `Cargo.lock`,
  `Cargo.toml.orig`, `.cargo_vcs_info.json` and the matching `[[example]]` /
  `[[test]]` targets from `Cargo.toml`. No other source is modified.

# @exp/ede

eDE's contract (#EDE-8): what a shell document is and how an edit reaches
the screen. Each `fixtures/*.json` is the spec (its `$comment`) plus cases;
`src/` holds the pure TypeScript reference that replays them; `ede-ui`
(`apps/ede/crates/ede-ui`, the eUI-on-disk runtime, #EDE-9) replays the same
fixtures in Rust, so the shell, the agent tools and the web harness agree.

| fixture | covers |
|---|---|
| `surface.json` | the `surface` header every document carries (a wlr-layer-shell surface: layer, anchor, size, exclusive zone, margin, keyboard; or a popup of another document), `normalizeSurface`; the Tier C rules of `documentIssues` (a document grants no privileges: no password inputs, host functions only from `surface.functions`, ids once) |
| `apply.json` | the apply state machine `reduce(state, event)`: validate, refuse on one issue, commit, reload, the shell's frame within 2 s or rollback, three crashes after an edit revert it, three more go to factory |

Not an SDK module: eDE lives in `apps/ede` (`docs/libraries.md`). Private, no release.

Licence: Apache-2.0

# `eagent-hub-core`: eAgent's orchestration reducer

A pure Rust port of `packages/eagent/src/reduce.ts` (#EXP-1509): the session
tree, the bus, wake-ups, end reports, caps and the watchdog as one function,

```rust
let outcome = eagent_hub_core::reduce(&state, &envelope);
// Outcome { state, effects, result } | Outcome { state (unchanged), effects: [], failure }
```

`state` is a `HubState`, `envelope` an eCore `Envelope` (`at` is now), the
effects are `post`, `edit`, `brief`, `nudge` and `kill`. Types mirror
`packages/eagent/src/model.ts` in camelCase JSON; `Field<T>` keeps a
TypeScript `undefined` apart from `null` where the fixtures do.
`hydrate(given)` fills a fixture's `given` with `rules.json` defaults.

**The rule** (`tests/dependency_gate.rs`): eCore, serde and serde_json only;
no I/O, no clock, no async.

**Byte-identical copy** with the TS reference: lines come from
`packages/eagent/fixtures/rules.json` (compiled in); the plan (2000) and
report (4000) clips count UTF-16 code units like JS `.slice`; `.trim()` uses
JavaScript's whitespace set; an event trigger's payload renders like
`JSON.stringify(payload, null, 2)` in source key order (`preserve_order`);
`resetsAt` parses like `Date.parse` of an RFC 3339 stamp (unparseable = the
wall never expires); orderings tie-break on createdAt, then id.

**Tests**: `cargo test -p eagent-hub-core` replays every case of
`packages/eagent/fixtures/*.json` (`tests/fixtures.rs`, the semantics of
`eagent.test.ts`) plus the pure `refusalCases`, `silentCases` and
`liveIdCases`. The WASM build ([`eagent-hub-core-wasm`](../eagent-hub-core-wasm))
replays them through `packages/eagent/src/replay.ts`.

Licence: Apache-2.0

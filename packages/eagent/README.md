# @exp/eagent

eAgent's orchestration spec (#EXP-1508): the session tree, the bus, wake-ups,
end reports, caps and the watchdog, extracted from Exponential's server
(`apps/web/src/lib/sessions`, `lib/agents/triggers.ts`). Each
`fixtures/*.json` is the spec (its `$comment`) plus scenario cases; `src/`
holds the pure TypeScript reference, `reduce(state, event) → { state,
effects, result | failure }`, that replays them. `eagent-hub-core`
(#EXP-1509, Rust + WASM) replays the same fixtures; the server's pure parts
and copy are drift-gated by `apps/web/src/lib/sessions/eagent-spec.test.ts`.

Shared types come from eCore: every event is an `Envelope` (unix ms `at`,
dotted `kind`), every id follows eCore's id rule, every refusal is a
`Failure` (`invalid`, `limit`, `conflict`, `not_found`).

| fixture | covers |
|---|---|
| `rules.json` | thresholds (caps, reuse, watchdog, triggers), the hub's copy, fixture defaults |
| `tree.json` | roots and children, the depth and live caps, claims and the first brief, revive and continue, End |
| `bus.json` | who hears a transition, what wakes, privacy and sealed lines, reports |
| `wakeups.json` | watchers, pending input, schedules (fold, re-arm, quiet ticks), event triggers |
| `watchdog.json` | unclaimed, nudges, stuck, offline, interruptions and their resume clock, usage walls, the heartbeat |
| `reuse.json` | when an agent's message reuses a thread or forks a fresh one |
| `incidents.json` | the field failures of 2026-10-30/31 (#FEED-172, #EXP-1500, interrupted runs, #FEED-178) |

Events: `send.start`, `send.message`, `send.watch`, `session.end`,
`session.pr_opened`, `device.claim`, `device.report`, `device.reply`,
`device.consume`, `device.heartbeat`, `watchdog.tick`, `trigger.schedule`,
`trigger.event`. Effects: `post`, `edit`, `brief`, `nudge`, `kill`.

Out of the core on purpose: cron matching (the host scheduler sends the due
minutes), I/O, the clock, sealing (the hub only sees the `sealed` flag and the
device's per-tree copies), access checks beyond visibility.

Licence: Apache-2.0

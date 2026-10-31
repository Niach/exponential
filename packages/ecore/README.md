# @exp/ecore

eCore's contract (#EXP-1504): `contract.json` is the one source for the ids,
the grant + token format, capability names, the delivery verdict, the error
shape and the event envelope. `bun run --filter @exp/ecore generate` writes
Rust (`apps/desktop/crates/ecore`), TS (`src/contract.generated.ts`), Swift
(`ExpCore`) and Kotlin (`com.exponential.app.ecore`); `fixtures/ecore.json`
replays on all four, and `apps/web/src/lib/codegen-drift.test.ts` fails a
stale output. Never: I/O, runtimes, network, UI, other modules (see the
crate's README).

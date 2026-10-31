# `eagent-hub-core-wasm`: eAgent's reducer for JavaScript hosts

The wasm-bindgen facade over [`eagent-hub-core`](../eagent-hub-core)
(#EXP-1509). JSON strings in and out:

```js
import { initSync, hydrate, reduce } from "./eagent_hub_core_wasm.js"
initSync({ module: wasmBytes })
let state = hydrate(JSON.stringify(given))               // HubState JSON
const outcome = JSON.parse(reduce(state, JSON.stringify(envelope)))
```

A malformed state or envelope throws; a refused event is an outcome with
`failure`.

## Build

`scripts/build-eagent-wasm.sh` (repo root) → `packages/eagent/wasm/`
(gitignored, `--target web`). `cd packages/eagent && bun run test:wasm`
replays every fixture through it with the TS reference's replayer
(`src/suite.ts`); `release-suite.yml` runs both as `eagent-wasm-replay`.
Needs the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the
`wasm-bindgen` version in `apps/desktop/Cargo.lock`.

Licence: Apache-2.0

// #EXP-1509: eagent-hub-core's WASM build replays every fixture through the
// same replayer and assertions as the TS reference (src/suite.ts). Build it
// first: scripts/build-eagent-wasm.sh (repo root) → packages/eagent/wasm/,
// or point EAGENT_HUB_WASM at another wasm-bindgen `--target web` output.
// Runs in release-suite.yml (`eagent-wasm-replay`), never in the PR gate.
import { describe, expect, test } from "bun:test"
import { existsSync } from "node:fs"
import { join, resolve } from "node:path"

import watchdog from "../fixtures/watchdog.json" with { type: "json" }
import type { HubEvent, HubState, Outcome } from "../src/model"
import { replayScenarios } from "../src/suite"

const dir = resolve(process.env.EAGENT_HUB_WASM ?? join(import.meta.dir, `../wasm`))
const glue = join(dir, `eagent_hub_core_wasm.js`)
if (!existsSync(glue)) throw new Error(`${glue} is missing: run scripts/build-eagent-wasm.sh first`)

const wasm = await import(glue)
wasm.initSync({ module: await Bun.file(join(dir, `eagent_hub_core_wasm_bg.wasm`)).arrayBuffer() })

const reduce = (state: HubState, event: HubEvent): Outcome => JSON.parse(wasm.reduce(JSON.stringify(state), JSON.stringify(event)))
const hydrate = (given: unknown): HubState => JSON.parse(wasm.hydrate(JSON.stringify(given)))

replayScenarios(reduce, hydrate)

describe(`wasm entrypoints`, () => {
  test(`a malformed envelope throws, a refused event is an outcome`, () => {
    const state = hydrate({ sessions: watchdog.cases[0]!.given.sessions })
    expect(() => wasm.reduce(JSON.stringify(state), `{}`)).toThrow()
    const outcome = reduce(state, { v: 1, id: `e1`, kind: `device.nope`, at: 0, source: `fixture`, sessionId: null, data: {} })
    expect(outcome.failure?.code).toBe(`invalid`)
    expect(outcome.effects).toEqual([])
  })
})

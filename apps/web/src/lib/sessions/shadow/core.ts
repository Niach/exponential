// #EXP-1510: eagent-hub-core (apps/desktop/crates/eagent-hub-core-wasm) for
// the server. scripts/build-eagent-wasm.sh emits it into packages/eagent/wasm/
// (gitignored; build-web.yml builds it before the image, the runtime stage
// copies packages/). Loaded once, lazily, from a plain file path the bundler
// never sees; absent (a dev checkout without the build) = the shadow is off.

import { existsSync, readFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { pathToFileURL } from "node:url"
import type { HubEvent, HubState, Outcome } from "@exp/eagent/model"

const GLUE = `eagent_hub_core_wasm.js`
const BYTES = `eagent_hub_core_wasm_bg.wasm`

type Bindings = { initSync: (opts: { module: BufferSource }) => unknown; reduce: (state: string, envelope: string) => string }

export interface Core {
  reduce: (state: HubState, event: HubEvent) => Outcome
}

/** Where the build may be: `EAGENT_HUB_WASM`, the image's /app/packages, a dev checkout from apps/web. */
export function coreDirs(cwd = process.cwd()): string[] {
  return [
    process.env.EAGENT_HUB_WASM,
    join(cwd, `packages/eagent/wasm`),
    join(cwd, `../../packages/eagent/wasm`),
  ].filter((dir): dir is string => !!dir).map((dir) => resolve(dir))
}

let loading: Promise<Core | null> | undefined

export function loadCore(): Promise<Core | null> {
  loading ??= (async () => {
    const dir = coreDirs().find((candidate) => existsSync(join(candidate, GLUE)) && existsSync(join(candidate, BYTES)))
    if (!dir) return null
    const wasm = (await import(/* @vite-ignore */ pathToFileURL(join(dir, GLUE)).href)) as Bindings
    wasm.initSync({ module: readFileSync(join(dir, BYTES)) })
    return {
      reduce: (state: HubState, event: HubEvent) => JSON.parse(wasm.reduce(JSON.stringify(state), JSON.stringify(event))) as Outcome,
    }
  })().catch((error) => {
    console.error(`[eagent-shadow] core failed to load`, error)
    return null
  })
  return loading
}

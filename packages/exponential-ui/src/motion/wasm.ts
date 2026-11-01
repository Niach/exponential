// EXP-1603: eUI Motion's engine in JS hosts: the committed WASM build of
// apps/desktop/crates/eui-motion-wasm (wasm/motion/, rebuilt by
// scripts/build-motion-wasm.sh). Servers load it synchronously (validation
// in exponential_show); browsers load it lazily (the Motion painters).
import * as bindings from "../../wasm/motion/eui_motion_wasm.js"

export type MotionIssue = { code: string; path: string; message: string }
export type MotionBindings = typeof bindings

let ready = false
let loading: Promise<MotionBindings> | null = null

/** Server/test hosts: instantiate synchronously from the file next to the bindings. */
export function motionWasmSync(): MotionBindings {
  if (ready) return bindings
  // Node/Bun only: a browser bundle must call `loadMotionWasm`.
  const { readFileSync } = require(`node:fs`) as typeof import("node:fs")
  const bytes = readFileSync(new URL(`../../wasm/motion/eui_motion_wasm_bg.wasm`, import.meta.url))
  bindings.initSync({ module: bytes })
  ready = true
  return bindings
}

/** Browsers: fetch and instantiate once (`url` = where the host serves the .wasm). */
export function loadMotionWasm(url?: string | URL): Promise<MotionBindings> {
  if (ready) return Promise.resolve(bindings)
  loading ??= bindings
    .default(url ? { module_or_path: url } : undefined)
    .then(() => {
      ready = true
      return bindings
    })
  return loading
}

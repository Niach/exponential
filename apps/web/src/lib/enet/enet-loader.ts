// EXP-1457: loads exponential-net-wasm on demand. scripts/build-enet-wasm.sh
// emits it into public/enet/ (CI builds it before the image), so it is a
// plain static file the bundler never sees: the import below is resolved by
// the browser at runtime, after sign-in, never part of the initial bundle.

import type { EnetModule, EnetNode } from "./web-install"

const ENET_JS = `/enet/exponential_net_wasm.js`

/** The generated wasm-bindgen module (scripts/build-enet-wasm.sh). */
type WasmBindings = {
  default: () => Promise<unknown>
  WasmNode: { start: (relayUrl: string, secretKey: Uint8Array) => Promise<EnetNode> }
  generateIdentity: () => Uint8Array
  pushPublicKey: (identity: Uint8Array) => Uint8Array
  identitySign: (identity: Uint8Array, message: Uint8Array) => Uint8Array
}

let loading: Promise<EnetModule> | undefined

export function loadEnet(): Promise<EnetModule> {
  loading ??= (async () => {
    const wasm = (await import(/* @vite-ignore */ ENET_JS)) as WasmBindings
    await wasm.default()
    return {
      generateIdentity: wasm.generateIdentity,
      pushPublicKey: wasm.pushPublicKey,
      identitySign: wasm.identitySign,
      startNode: (relayUrl, identity) => wasm.WasmNode.start(relayUrl, identity),
    }
  })()
  // A failed load (no artifact in a dev checkout, offline) may retry later.
  loading.catch(() => {
    loading = undefined
  })
  return loading
}

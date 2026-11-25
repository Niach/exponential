import * as bindings from "../../../exponential-ui/wasm/motion/eui_motion_wasm.js"
import { MOTION_WASM_GZ_BASE64 } from "../../../exponential-ui/wasm/motion/bytes"

// #NEW-140: genUI's `motion/wasm` in the genUI document (vite.config.ts
// `leanMotionWasm`): the same engine from its gzipped bytes, inline. Nothing
// is fetched: the document is under OpenClaw's 2 MiB cap, and the sandbox's
// `connect-src` refuses a `data:` URL.

let loading: Promise<typeof bindings> | null = null

export function loadMotionWasm(): Promise<typeof bindings> {
  loading ??= (async () => {
    const packed = Uint8Array.from(atob(MOTION_WASM_GZ_BASE64), (c) => c.charCodeAt(0))
    const bytes = await new Response(new Blob([packed]).stream().pipeThrough(new DecompressionStream(`gzip`))).arrayBuffer()
    await bindings.default({ module_or_path: bytes })
    return bindings
  })()
  return loading
}

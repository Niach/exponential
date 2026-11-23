// #AXC-12 (VAPP-296): `@exponential_app/eui/express`, the Express inference
// format on the web: the committed WASM build of a2ui-express
// (wasm/express/, the SAME rev eui-core pins; rebuilt by
// scripts/build-express-wasm.sh) over the core catalog. A lazy chunk: the
// transports import it the moment a stream carries Express; a host may load
// it up front (`loadExpress()`). Loading registers the compiler with the
// host decoders (`ExpressDecoder`, the SSE / WebSocket / MCP Express paths).
import * as bindings from "../../wasm/express/a2ui_express.js"
import type { Catalogs, ExpressError as WasmExpressError } from "../../wasm/express/a2ui_express.js"
import coreSchema from "../../catalog/core.schema.json" with { type: "json" }
import { EXPRESS_DEFAULT_SURFACE_ID } from "../host/contract"
import { registerExpressEngine, type ExpressEngine, type ExpressStream } from "../host/decoders"
import { expressCatalog } from "./catalog"

export { ExpressDecoder, decodeExpressBody, expressLoaded, inferenceFormatOf } from "../host/decoders"
export type { ExpressEngine, ExpressPart, ExpressStream } from "../host/decoders"
export { expressCatalog } from "./catalog"

/** One Express problem: the compiler's category (`ParseError`,
 *  `ValidationError`, …) and the message. */
export type ExpressError = WasmExpressError

let catalogs: Catalogs | undefined
let loading: Promise<void> | null = null

function ready(): void {
  catalogs ??= new bindings.Catalogs(expressCatalog(coreSchema) as Record<string, unknown>, { surfaceId: EXPRESS_DEFAULT_SURFACE_ID })
  const loaded = catalogs
  const engine: ExpressEngine = {
    stream: ({ surfaceId, wrapped }) => loaded.stream({ surfaceId, wrapped, afterRejectedBlock: `continue` }) as unknown as ExpressStream,
  }
  registerExpressEngine(engine)
}

/** Instantiate synchronously from the module bytes (servers, tests). */
export function initExpress(bytes: BufferSource): void {
  if (catalogs) return
  bindings.initSync({ module: bytes })
  ready()
}

/** Browsers: fetch and instantiate once (`url` = where the host serves the
 *  .wasm; default beside this module), then register the compiler. */
export function loadExpress(url?: string | URL): Promise<void> {
  if (catalogs) return Promise.resolve()
  loading ??= bindings.default(url ? { module_or_path: url } : undefined).then(ready)
  return loading
}

function loaded(): Catalogs {
  if (!catalogs) throw new Error(`Express is not loaded (loadExpress())`)
  return catalogs
}

/** One Express block (`<a2ui>` tags optional) → A2UI messages; statements
 *  before any `surface("…")` line target `surfaceId`. Throws an
 *  ExpressError. */
export function compileExpress(source: string, surfaceId: string = EXPRESS_DEFAULT_SURFACE_ID): unknown[] {
  return loaded().compile(source, { surfaceId })
}

/** A2UI messages → one Express block (without the `<a2ui>` tags), keyword
 *  arguments, components in message order (eui-core `express::decompile`). */
export function decompileExpress(messages: unknown[]): string {
  return loaded().decompile(messages as Record<string, unknown>[], { keywordArgs: true, order: `upstream` })
}

/** The Express section of an agent's system prompt over the core catalog
 *  WITH its prose, compact style (eui-core `express::express_prompt`). */
export function expressPrompt(): string {
  loaded()
  return bindings.generatePrompt(coreSchema as Record<string, unknown>, { style: `compact` })
}


// EXP-1603: what exponential_show (and any host taking Motion from a model)
// checks before accepting a Motion node: the inline dialect compiles, inline
// Lottie passes the engine's validator (expressions refused, the limits),
// `src` names an attachment. Pure over the synchronously loaded WASM.
import { gunzipSync } from "node:zlib"
import { MOTION_WASM_GZ_BASE64 } from "../../wasm/motion/bytes"
import { initMotionWasm, type MotionBindings, type MotionIssue } from "./wasm"

/** Servers and tests: instantiate synchronously from the embedded module. */
export function motionWasmSync(): MotionBindings {
  return initMotionWasm(gunzipSync(Buffer.from(MOTION_WASM_GZ_BASE64, `base64`)))
}

/** The dialect fields a Motion node carries inline (its props are the document). */
export const MOTION_DIALECT_KEYS = [`size`, `fps`, `duration`, `start`, `time`, `name`, `layers`, `assets`, `markers`, `lottie`] as const

const ATTACHMENT_SRC = /^\/api\/attachments\/[0-9a-zA-Z-]+$/

function describe(issues: MotionIssue[]): string[] {
  return issues.map((issue) => (issue.path ? `${issue.code} at ${issue.path}: ${issue.message}` : `${issue.code}: ${issue.message}`))
}

/** Problems with one Motion node's props (empty = accepted). */
export function motionPropsProblems(props: Record<string, unknown>): string[] {
  const wasm = motionWasmSync()
  const sources = [`layers`, `lottie`, `src`].filter((key) => props[key] !== undefined)
  if (sources.length !== 1) return [`a Motion needs exactly one of layers (the animation), lottie or src`]
  if (props.src !== undefined) {
    return typeof props.src === `string` && ATTACHMENT_SRC.test(props.src)
      ? []
      : [`src must be an attachment (/api/attachments/{id}); put the animation inline as layers instead`]
  }
  let lottie: unknown = props.lottie
  if (props.layers !== undefined) {
    const doc: Record<string, unknown> = {}
    for (const key of MOTION_DIALECT_KEYS) if (key !== `lottie` && props[key] !== undefined) doc[key] = props[key]
    const out = JSON.parse(wasm.compile(JSON.stringify(doc))) as { lottie?: unknown; errors?: MotionIssue[] }
    if (out.errors) return describe(out.errors)
    lottie = out.lottie
  }
  const report = JSON.parse(wasm.validate(JSON.stringify(lottie))) as { errors: MotionIssue[] }
  return describe(report.errors)
}

/** Compile a Motion node's inline dialect to Lottie (null when it has none or it fails). */
export function compileMotionProps(props: Record<string, unknown>): unknown | null {
  if (props.layers === undefined) return (props.lottie as unknown) ?? null
  const doc: Record<string, unknown> = {}
  for (const key of MOTION_DIALECT_KEYS) if (key !== `lottie` && props[key] !== undefined) doc[key] = props[key]
  const out = JSON.parse(motionWasmSync().compile(JSON.stringify(doc))) as { lottie?: unknown }
  return out.lottie ?? null
}

/** Round 8: problems with one Canvas node's `{size, shapes}` (empty =
 *  accepted): the engine's refusals (`CanvasPlayer`'s constructor) with
 *  their paths. Bound `shapes` (a binding or call) are checked when the
 *  data arrives, by the painter. */
export function canvasPropsProblems(props: Record<string, unknown>): string[] {
  const shapes = props.shapes
  if (shapes !== null && typeof shapes === `object` && !Array.isArray(shapes)) return []
  const wasm = motionWasmSync()
  try {
    new wasm.CanvasPlayer(JSON.stringify({ size: props.size, shapes })).free()
    return []
  } catch (e) {
    let issues: MotionIssue[] = []
    try {
      issues = JSON.parse(String(e)) as MotionIssue[]
    } catch {
      return [String(e)]
    }
    return describe(issues)
  }
}

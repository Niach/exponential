// VAPP-289 (round 9 §6.9): the answer to a `callRendererFunction` that passed
// the router's checks (its `call` op). The Rust core's `answer_renderer_call`
// mirrors it (Swift and Kotlin through the FFI).

import { evaluateCall } from "../dynamic"
import { rendererFunctionResponse } from "./contract"
import type { ClientMessage, HostOp } from "./contract"

/** The call at the AGENT position: a `rendererOnly` function reached through
 *  its args or a `${…}` expression answers INVALID_FUNCTION_CALL, any other
 *  failing evaluation EXECUTION_ERROR. `data` = the surface's (`{}` without). */
export function answerRendererCall(op: Extract<HostOp, { op: `call` }>, data: unknown, available?: readonly string[]): ClientMessage {
  const { value, failure } = evaluateCall({ "@call": op.name, catalogId: op.catalogId, args: op.args }, data, { catalogId: op.catalogId, position: `agent`, ...(available ? { available } : {}) })
  if (!failure) return rendererFunctionResponse(op.functionCallId, { value })
  return rendererFunctionResponse(op.functionCallId, { error: { code: failure.code === `INVALID_FUNCTION_CALL` ? failure.code : `EXECUTION_ERROR`, message: failure.message } })
}

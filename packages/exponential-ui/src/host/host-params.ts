// VAPP-206 (spec docs/spec/apps.md §4.1): the HOST-PROVIDED binding params.
// A template binding or a named query reads `${today}`, `${now}` and
// `${month}` (the team zone, read at each bind), so a query bound to today has
// rows before any event fires. Every host adds them to its `applyTemplate` and
// `bindDataModel` messages (`hostParams`); the pure router fills them. A
// binding's own param of the same name wins in a named query. Rust twin
// `eui_core::host::host_params`; `fixtures/host-router.json` locks them.

import { evaluateExpression } from "../cel"
import type { ExpressionEnv } from "../cel"

export type HostParams = Record<`today` | `now` | `month`, string>

/** The params at `now` (epoch ms; default the wall clock) in the team zone
 *  (minutes east of UTC at an instant; default UTC). */
export function hostParams(now?: number, offset?: ExpressionEnv[`offset`]): HostParams {
  const env: ExpressionEnv = { ...(now === undefined ? {} : { now }), ...(offset ? { offset } : {}) }
  const read = (expr: string): string => {
    const out = evaluateExpression(expr, {}, env)
    return `value` in out && typeof out.value === `string` ? out.value : ``
  }
  const today = read(`today()`)
  return { today, now: read(`now()`), month: today.slice(0, 7) }
}

/** A source's `${today}`, `${now}`, `${month}` from the host's params
 *  (percent-encoded like a query value). Without them it stays as written. */
export function fillHostParams(source: string, params: unknown): string {
  if (!params || typeof params !== `object`) return source
  let out = source
  for (const name of [`today`, `now`, `month`]) {
    const v = (params as Record<string, unknown>)[name]
    if (typeof v === `string`) out = out.split(`\${${name}}`).join(encodeURIComponent(v))
  }
  return out
}

/** The message with `hostParams` in its `applyTemplate` or `bindDataModel`
 *  body; any other message as it is. */
export function withHostParams(message: unknown, params: HostParams): unknown {
  if (!message || typeof message !== `object` || Array.isArray(message)) return message
  const m = message as Record<string, unknown>
  for (const kind of [`applyTemplate`, `bindDataModel`]) {
    const body = m[kind]
    if (body && typeof body === `object` && !Array.isArray(body)) return { ...m, [kind]: { ...body, hostParams: params } }
  }
  return message
}

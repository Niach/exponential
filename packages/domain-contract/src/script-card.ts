// EXP-1279 (EXP-1259 D13): a code-mode call (`mcp__codemode__exec`, contract
// toolKind `script`) renders as ONE card — "Ran a script · 59 calls · 3.0 s"
// — with the calls it made grouped by tool, the script itself behind a
// "Script" disclosure. Pure model, byte-locked by `fixtures/script-card.json`,
// replayed by desktop `steer::script_card`, iOS `ExpCore ScriptCard.swift`,
// Android `domain/ScriptCard.kt` and this file's test.

import contractJson from "../contract.json" with { type: "json" }
import fixture from "../fixtures/script-card.json" with { type: "json" }
import { expToolIndex } from "./exp-tool-group"
import { workDurationShort } from "./work-card"

const tools = (contractJson as unknown as {
  expToolDisplay: { tools: { name: string; done: string }[] }
}).expToolDisplay.tools

export const SCRIPT_CARD_COPY = fixture.copy
export const SCRIPT_CARD_SEPARATOR = ` · `

/** One entry of codemode's call log (`_meta["exponential/calls"]`). */
export interface ScriptCall {
  /** `<server>.<tool>` as the script called it. */
  tool: string
  subject?: string | null
  ok: boolean
  ms?: number | null
}

export interface ScriptCardInput {
  running: boolean
  /** Total calls the script made (the log is capped). */
  count?: number | null
  calls?: readonly ScriptCall[] | null
  elapsedMs?: number | null
}

export interface ScriptCardGroup {
  tool: string
  /** An Exponential tool (draw the brand mark), else the generic tool glyph. */
  exponential: boolean
  label: string
  subject: string | null
  /** `+54`, or null. */
  more: string | null
  /** `×55`, or null; replaced by `failed` when calls failed. */
  count: string | null
  /** `1 failed` (rose), or null. */
  failed: string | null
}

export interface ScriptCard {
  title: string
  meta: string | null
  groups: ScriptCardGroup[]
  /** `12 more calls`: the log's cap cut these off. */
  overflow: string | null
}

/** `3.0 s` under a minute, the Work card's coarse format above. */
export function scriptDuration(ms: number): string {
  return ms < 60_000 ? `${(Math.max(0, ms) / 1000).toFixed(1)} s` : workDurationShort(ms)
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`
}

export function scriptCard(input: ScriptCardInput): ScriptCard {
  if (input.running) {
    return { title: fixture.copy.running, meta: null, groups: [], overflow: null }
  }
  const calls = input.calls ?? []
  const count = Math.max(input.count ?? calls.length, calls.length)
  const meta: string[] = []
  if (count > 0) meta.push(plural(count, fixture.copy.call, fixture.copy.calls))
  if (typeof input.elapsedMs === `number`) meta.push(scriptDuration(input.elapsedMs))

  const order: string[] = []
  const byTool = new Map<string, ScriptCall[]>()
  for (const call of calls) {
    const key = call.tool.trim()
    if (!byTool.has(key)) {
      byTool.set(key, [])
      order.push(key)
    }
    byTool.get(key)!.push(call)
  }
  const groups = order.map((tool): ScriptCardGroup => {
    const members = byTool.get(tool)!
    const name = tool.slice(tool.lastIndexOf(`.`) + 1)
    const index = expToolIndex(name)
    const subject = members.map((call) => (call.subject ?? ``).trim()).find(Boolean) ?? null
    const failed = members.filter((call) => !call.ok).length
    return {
      tool,
      exponential: index !== null,
      label: index !== null ? tools[index].done : tool,
      subject,
      more: subject && members.length > 1 ? `+${members.length - 1}` : null,
      count: failed === 0 && members.length > 1 ? `×${members.length}` : null,
      failed: failed > 0 ? `${failed} ${fixture.copy.failed}` : null,
    }
  })
  const cut = count - calls.length
  return {
    title: fixture.copy.done,
    meta: meta.length > 0 ? meta.join(SCRIPT_CARD_SEPARATOR) : null,
    groups,
    overflow: cut > 0 ? `${cut} ${cut === 1 ? fixture.copy.moreCall : fixture.copy.moreCalls}` : null,
  }
}

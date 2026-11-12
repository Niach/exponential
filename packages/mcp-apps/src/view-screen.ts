// #VAPP-292 — what ONE call of a view-bound tool draws. A host binds the view
// to the whole noun tool (`exponential_issues`, every op), so the view reads
// the call's `op` (ui/notifications/tool-input) before it draws:
//   - a list op → the list, its rows from `structuredContent.items` (the
//     server keeps the model's text a summary, lib/emcp/view-results.ts), or
//     the JSON text of an older server;
//   - an op that answers ONE issue (get, create, update, update_status) → the
//     issue itself (get's answer is the detail; the others reload it);
//   - anything else (delete, labels, subscribe, mark_read, a sign-in…) →
//     nothing: the frame stays zero height and the host shows the call only.
// The display mode it opens in: `show` (the person asked to SEE their work)
// and a team app open full size, every other call stays inline.

import type { DisplayMode, ToolResult } from "./bridge"
import { decodeToolResult, type McpAppView } from "./model"

/** The page a list screen asks for next. */
export interface ListPage {
  offset: number
  limit: number
  nextOffset: number | null
}

export type CallScreen =
  | { kind: `list`; rows: unknown[]; page: ListPage | null }
  | { kind: `issue`; id: string; detail: unknown | null }
  | { kind: `none` }
  | { kind: `error`; message: string }

const LIST_OPS: Partial<Record<McpAppView, readonly string[]>> = {
  issues: [`list`, `show`],
  inbox: [`list`],
  devices: [`list`],
}

const ISSUE_OPS = new Set([`get`, `create`, `update`, `update_status`])

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === `object` && value !== null && !Array.isArray(value)

function asPage(value: unknown): ListPage | null {
  if (!isObject(value)) return null
  const { offset, limit, nextOffset } = value
  if (typeof offset !== `number` || typeof limit !== `number`) return null
  return { offset, limit, nextOffset: typeof nextOffset === `number` ? nextOffset : null }
}

/** A list answer's rows and page, whichever server wrote it. */
export function listRows(result: ToolResult): { rows: unknown[]; page: ListPage | null } | null {
  const structured = result.structuredContent
  if (isObject(structured) && Array.isArray(structured.items)) {
    return { rows: structured.items, page: asPage(structured.page) }
  }
  const decoded = decodeToolResult<unknown>(result)
  if (decoded.kind === `ok` && Array.isArray(decoded.data)) return { rows: decoded.data, page: null }
  return null
}

/** What a call of `view`'s tool draws. `op` is the call's own (absent on a
 *  host that sends no tool input: the answer's shape decides). */
export function screenForCall(view: McpAppView, op: string | undefined, result: ToolResult): CallScreen {
  const listOps = LIST_OPS[view] ?? []
  if (op !== undefined && !listOps.includes(op) && !(view === `issues` && ISSUE_OPS.has(op))) {
    return { kind: `none` }
  }
  if (result.isError) {
    // A failed list still says why; a failed write is the chat's to tell.
    if (op === undefined || listOps.includes(op) || op === `get`) {
      const decoded = decodeToolResult<unknown>(result)
      return { kind: `error`, message: decoded.kind === `error` ? decoded.message : `The tool call failed.` }
    }
    return { kind: `none` }
  }
  const list = listRows(result)
  if (list && (op === undefined || listOps.includes(op))) return { kind: `list`, ...list }
  if (view === `issues`) {
    const decoded = decodeToolResult<unknown>(result)
    if (decoded.kind === `ok` && isObject(decoded.data)) {
      const data = decoded.data
      // update_status answers `{issue}` on some paths.
      const issue = isObject(data.issue) ? data.issue : data
      if (typeof issue.id === `string` && typeof issue.identifier === `string`) {
        return { kind: `issue`, id: issue.id, detail: op === `get` ? issue : null }
      }
    }
  }
  return { kind: `none` }
}

/** The mode a call opens in (`inline` = stay in the conversation). */
export function defaultDisplayMode(view: McpAppView, op: string | undefined): DisplayMode {
  return view === `issues` && op === `show` ? `fullscreen` : `inline`
}

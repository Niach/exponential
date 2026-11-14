// #NEW-18: the duplicate-start warning (fixtures/overlap.json). Pure; the
// server gathers the inputs (lib/sessions/issue-overlap.ts).

import overlap from "../fixtures/overlap.json" with { type: "json" }
import { fill } from "./copy"

export { overlap }

export interface OverlapIssue {
  identifier: string
  /** done, cancelled or duplicate */
  done: boolean
  /** live sessions holding it outside the send's tree, oldest first */
  holders: readonly { sessionId: string; visible: boolean }[]
}

export function overlapWarning(issues: readonly OverlapIssue[]): string | null {
  const seen = new Set<string>()
  const lines: string[] = []
  for (const entry of issues) {
    if (seen.has(entry.identifier)) continue
    seen.add(entry.identifier)
    if (entry.done) {
      lines.push(fill(overlap.copy.done, { issue: entry.identifier }))
      continue
    }
    const visible = entry.holders.filter((holder) => holder.visible).map((holder) => holder.sessionId)
    if (visible.length) lines.push(fill(overlap.copy.held, { issue: entry.identifier, ids: visible.join(`, `) }))
    else if (entry.holders.length) lines.push(fill(overlap.copy.heldPrivate, { issue: entry.identifier }))
  }
  return lines.length ? lines.join(` `) : null
}

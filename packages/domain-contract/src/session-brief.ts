// EXP-1291 Threads v2: the server-composed FIRST prompt of every new session
// (`sessions.brief`). Pure and byte-locked by `fixtures/session-brief.json`.
// The server is the only caller; the device reads the result.

import fixture from "../fixtures/session-brief.json" with { type: "json" }
import contractJson from "../contract.json" with { type: "json" }

export interface BriefInput {
  /** The person's text (the composer, an agent's spawn request …). */
  text?: string | null
  issues?: { identifier: string; title: string }[]
  /** A CHILD session's parent. */
  parent?: { title: string; depth: number; branch: string | null } | null
  /** EXP-1280: a session started in plan mode. */
  planMode?: boolean
}

function cutToBytes(text: string, maxBytes: number): string {
  const encoder = new TextEncoder()
  if (encoder.encode(text).length <= maxBytes) return text
  const lines = text.split(`\n`)
  const kept: string[] = []
  let size = encoder.encode(`\n…`).length
  for (const line of lines) {
    const next = encoder.encode(line).length + (kept.length > 0 ? 1 : 0)
    if (size + next > maxBytes) break
    kept.push(line)
    size += next
  }
  return `${kept.join(`\n`)}\n…`
}

export function composeBrief(input: BriefInput): string {
  const sections: string[] = []
  const section = (heading: string, body: string) => {
    if (body.trim()) sections.push(`## ${heading}\n\n${body}`)
  }
  section(fixture.headings.request, (input.text ?? ``).trim())
  // The plan gate rides right behind the Request: the cut at `briefMaxBytes`
  // trims from the END, so a near-cap request can never take it.
  if (input.planMode) section(fixture.headings.plan, fixture.plan)
  section(
    fixture.headings.issues,
    (input.issues ?? []).map((issue) => `- ${issue.identifier} ${issue.title}`).join(`\n`)
  )
  if (input.parent) {
    const template = input.parent.branch ? fixture.parent : fixture.parentNoBranch
    section(
      fixture.headings.parent,
      template
        .replaceAll(`{title}`, input.parent.title)
        .replaceAll(`{depth}`, String(input.parent.depth))
        .replaceAll(`{maxDepth}`, String(contractJson.sessionsSend.treeMaxDepth))
        .replaceAll(`{branch}`, input.parent.branch ?? ``)
    )
  }
  if (sections.length === 0) return fixture.empty
  return cutToBytes(sections.join(`\n\n`), contractJson.sessionsSend.briefMaxBytes)
}

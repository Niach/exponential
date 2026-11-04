// EXP-1291 Threads v2: the server-composed FIRST prompt of every new session
// (`sessions.brief`). Pure and byte-locked by `fixtures/session-brief.json`.
// The server is the only caller; the device reads the result.

import fixture from "../fixtures/session-brief.json" with { type: "json" }
import contractJson from "../contract.json" with { type: "json" }

/** EXP-1584: an issue as a brief names it (its description rides along). */
export interface BriefIssue {
  identifier: string
  title: string
  description?: string | null
}

export interface BriefInput {
  /** The person's text (the composer, an agent's spawn request …). */
  text?: string | null
  issues?: BriefIssue[]
  /** EXP-1686: a new agent conversation holds no rows: its first message's
   *  `issue_brief` is the Issues section, as stored. */
  issueBrief?: string | null
  /** A CHILD session's parent. */
  parent?: { title: string; depth: number; branch: string | null } | null
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

/** The Issues section's body: the `- <identifier> <title>` lines, then
 *  (EXP-1584) one `### <identifier>` block per issue with a description. */
function issuesBody(issues: BriefIssue[]): string {
  const lines = issues.map((issue) => `- ${issue.identifier} ${issue.title}`).join(`\n`)
  const details = issues.flatMap((issue) => {
    const description = issue.description?.trim()
    return description ? [`### ${issue.identifier}\n\n${description}`] : []
  })
  return [lines, ...details].filter(Boolean).join(`\n\n`)
}

/** EXP-1584 (`issueBriefCases`): the Issues section a message forwarding
 *  `issues` into a session already under way carries (`issue_brief`); the
 *  device appends it to what the agent reads. No issues = null. */
export function composeIssueBrief(issues: BriefIssue[]): string | null {
  if (issues.length === 0) return null
  return cutToBytes(`## ${fixture.headings.issues}\n\n${issuesBody(issues)}`, contractJson.sessionsSend.briefMaxBytes)
}

/** EXP-1584 (`deliveryCases`): the text the agent reads for a message with
 *  an `issue_brief` (the device's half; this is its reference). */
export function withIssueBrief(body: string, issueBrief: string | null | undefined): string {
  const brief = issueBrief?.trim()
  if (!brief) return body
  const text = body.trimEnd()
  return text ? `${text}\n\n${brief}` : brief
}

/** EXP-1686 (`forwardTextCases`): the person's text of a forward to an agent,
 *  the refs first, composed before the client seals it: the bubble is their
 *  record of what was sent. */
export function agentForwardText(text: string, identifiers: readonly string[]): string {
  if (identifiers.length === 0) return text
  const refs = identifiers.map((identifier) => `#${identifier}`).join(` `)
  const body = text.trim()
  return body ? `${refs}\n\n${body}` : refs
}

export function composeBrief(input: BriefInput): string {
  const sections: string[] = []
  const section = (heading: string, body: string) => {
    if (body.trim()) sections.push(`## ${heading}\n\n${body}`)
  }
  section(fixture.headings.request, (input.text ?? ``).trim())
  section(fixture.headings.issues, issuesBody(input.issues ?? []))
  if (input.issueBrief?.trim()) sections.push(input.issueBrief.trim())
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

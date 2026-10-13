// EXP-740/1291: what a session IS, resolved once for every surface that
// names one (tab strip, session header, lists). Pure: no queries, no React.
// The derivation is the server's (`deriveSessionTitle`, lib/sessions/
// brief.ts): title ?? action name ?? agent name ?? the first issue.

import type { Issue, Session } from "@/db/schema"

export interface SessionIdentity {
  /** The first linked issue's identifier, else null. */
  identifier: string | null
  /** The session's display title. */
  subject: string
}

type IdentitySession = Pick<Session, `title` | `actionName` | `name`>

export function sessionIdentity(row: {
  session: IdentitySession
  /** The thread's first linked issue, when the caller has it synced. */
  issue: Pick<Issue, `identifier` | `title`> | undefined
}): SessionIdentity {
  const { session, issue } = row
  const subject =
    session.title?.trim() ||
    session.actionName?.trim() ||
    session.name?.trim() ||
    issue?.title.trim() ||
    `Untitled`
  return { identifier: issue?.identifier ?? null, subject }
}

/** The phase a tab's tooltip spells out. */
export function tabPhaseLabel(row: {
  session: Pick<Session, `status` | `needsInput`>
  device: { label: string | null }
  paused: boolean
}): string {
  const { session, device, paused } = row
  if (paused) return `Paused · ${device.label ?? `the device`} is offline`
  if (session.status === `ended`) return `Session ended`
  if (session.needsInput) {
    return device.label ? `Needs your input · ${device.label}` : `Needs your input`
  }
  if (session.status === `queued`) {
    return device.label ? `Queued · ${device.label}` : `Queued`
  }
  return device.label ? `Live · ${device.label}` : `Live`
}

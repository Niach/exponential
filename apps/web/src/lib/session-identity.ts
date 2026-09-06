// EXP-740: what a coding run IS, resolved once and rendered identically by
// the dock tab strip, the session route's header and the chat page — the
// three surfaces that used to hold their own copy of this inside
// agent-dock.tsx. Pure: no queries, no React, so it stays unit-testable.

import type { CodingSession, Issue } from "@/db/schema"
import { BUILTIN_CHAT_NAME } from "@/lib/builtin-actions"

/** EXP-688: one run's identity — the mono identifier (absent for action,
 * chat and batch runs) and its human subject. */
export interface SessionIdentity {
  /** `EXP-688` — null for action, chat, batch and not-yet-synced issue runs. */
  identifier: string | null
  /** The issue title, an action's name snapshot, `Chat`, or `Batch run`. */
  subject: string
}

/** The columns any of these predicates read — so a caller may pass a whole
 * `CodingSession`, a synthesized row, or a fixture. */
type IdentitySession = Pick<
  CodingSession,
  `issueId` | `actionId` | `actionName`
>

/** EXP-739: the hidden "Chat" builtin's runs. It has no DB `actions` row (so
 * `action_id` is NULL like a batch run's) and links no issue — the reserved
 * `actionName` snapshot is what tells the two apart. ×4 with the desktop's
 * `ActionRunKind::Chat` and the natives' chat rows. */
export function isChatSession(session: IdentitySession): boolean {
  return (
    session.issueId == null &&
    session.actionId == null &&
    session.actionName === BUILTIN_CHAT_NAME
  )
}

/** EXP-688: the run's identity. Action runs keep their name snapshot even if
 * the action was deleted (EXP-253); an issue-scoped row whose issue hasn't
 * synced yet says so. */
export function sessionIdentity(row: {
  session: IdentitySession
  issue: Pick<Issue, `identifier` | `title`> | undefined
}): SessionIdentity {
  const { session, issue } = row
  if (issue) {
    return {
      identifier: issue.identifier,
      subject: issue.title.trim() || `Untitled issue`,
    }
  }
  if (isChatSession(session)) {
    return { identifier: null, subject: BUILTIN_CHAT_NAME }
  }
  if (!session.issueId) {
    return {
      identifier: null,
      subject: session.actionName ?? `Batch run`,
    }
  }
  return { identifier: null, subject: `Issue syncing…` }
}

/** EXP-688: the phase a tab's tooltip spells out — the dock tab shows the
 * identity, so "Live · macbook" moves into the hover title. Mirrors the
 * session view's `phaseLabel` for the states a synced row can tell apart (a
 * tab has no viewer connection of its own). */
export function tabPhaseLabel(row: {
  session: Pick<CodingSession, `status` | `needsInput`>
  device: { label: string | null }
  paused: boolean
}): string {
  const { session, device, paused } = row
  if (paused) return `Paused · ${device.label ?? `the device`} is offline`
  if (session.status === `ended`) return `Session ended`
  if (session.needsInput) {
    return device.label
      ? `Needs your input · ${device.label}`
      : `Needs your input`
  }
  return device.label ? `Live · ${device.label}` : `Live`
}

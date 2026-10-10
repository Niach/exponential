// Pure, hook-free phrase helpers for issue_events rows (EXP-759). Shared by
// the product timeline (components/comment-rows/event.tsx) and the admin team
// activity log, so a status change reads the same on both.

// EXP-314: `status_changed` payloads carry the human status NAMES
// (`fromName`/`toName`) alongside the legacy enum anchors, so a custom status
// reads as itself. Rows written before EXP-314 have no names — fall back to
// the enum munge. Retired enum tokens keep their historic label (EXP-685:
// `todo` is gone from the vocabulary, but old events still name it); iOS
// EventPhrases, Android labelFor and desktop timeline.rs mirror this map.
import { estimateEventPhrase } from "@/lib/issue-estimate"
import { issueStatusOptions, issueStatusValues } from "@/lib/domain"
import type { StatusResolvable } from "@/lib/team-statuses"

export const RETIRED_STATUS_LABELS: Record<string, string> = { todo: `Todo` }

// A name-less legacy row reads as the builtin status row's display name ("In
// Progress"), the name the app shows for it ×4 — never a lowercase munge.
const BUILTIN_STATUS_LABELS: Record<string, string> = Object.fromEntries(
  issueStatusOptions.map((option) => [option.value, option.label])
)

/** The team's status resolver (`useTeamStatusesContext().resolve`): the
 *  statusId row, else the row anchored on the enum, else a constructed
 *  default. Only `id` and `name` are read here. */
export type StatusRowResolver = (
  issue: StatusResolvable
) => { id: string; name: string }

// A side's display name ×4 (desktop timeline.rs `status_display_name`): the
// payload's name snapshot, else the TEAM row its statusId names, else the
// team's row for a known enum anchor (a renamed "In Progress" reads as that
// name), else the retired-label / builtin-label / munge fallback for a wire
// value no row answers to. Without a resolver (the admin log) the anchor
// reads as the builtin label.
export function statusLabel(
  payload: Record<string, unknown>,
  side: `to` | `from`,
  resolve?: StatusRowResolver
): string {
  const name = payload[side === `to` ? `toName` : `fromName`]
  if (typeof name === `string` && name.length > 0) return name
  const token = String(payload[side] ?? ``)
  if (resolve) {
    const statusId = payload[side === `to` ? `toStatusId` : `fromStatusId`]
    if (typeof statusId === `string` && statusId.length > 0) {
      const row = resolve({ status: token, statusId })
      if (row.id === statusId) return row.name
    }
    if ((issueStatusValues as readonly string[]).includes(token)) {
      return resolve({ status: token, statusId: null }).name
    }
  }
  return (
    RETIRED_STATUS_LABELS[token] ??
    BUILTIN_STATUS_LABELS[token] ??
    token.replace(/_/g, ` `)
  )
}

// Priority wire values render capitalized ("urgent" → "Urgent"); anything
// unexpected falls back to the raw string.
export function priorityLabel(value: unknown): string {
  const raw = String(value ?? ``)
  return raw ? raw.charAt(0).toUpperCase() + raw.slice(1) : `None`
}

// One plain-text phrase per event row for compact logs (the admin team
// activity card). The product timeline renders richer JSX with the same
// vocabulary; keep the two in step when adding an event type.
export function issueEventPhrase(
  type: string,
  payload: Record<string, unknown> | null,
  resolve?: StatusRowResolver
): string {
  const p = payload ?? {}
  switch (type) {
    case `created`:
      return `created`
    case `status_changed`: {
      // ×4: `changed status from {from} to {to}`.
      const to = statusLabel(p, `to`, resolve)
      const from = statusLabel(p, `from`, resolve)
      if (!to) return `changed status`
      return from ? `changed status from ${from} to ${to}` : `changed status to ${to}`
    }
    case `assignee_changed`:
      return p.to ? `changed the assignee` : `removed the assignee`
    case `label_added`:
      return `added a label`
    case `label_removed`:
      return `removed a label`
    case `priority_changed`:
      return `set priority to ${priorityLabel(p.to)}`
    case `estimate_changed`:
      return estimateEventPhrase(p)
    case `board_moved`:
      return `moved to another board`
    case `pr_opened`:
      return `opened a pull request`
    case `pr_merged`:
      return `merged the pull request`
    case `relation_added`:
      return typeof p.relatedIdentifier === `string`
        ? `linked ${p.relatedIdentifier}`
        : `linked an issue`
    case `relation_removed`:
      return typeof p.relatedIdentifier === `string`
        ? `unlinked ${p.relatedIdentifier}`
        : `unlinked an issue`
    default:
      return type.replace(/_/g, ` `)
  }
}

// Actor fallback for creator-less rows: widget-filed issues have no user.
export function issueEventActorFallback(
  payload: Record<string, unknown> | null
): string {
  return payload?.source === `widget` ? `Widget` : `Someone`
}

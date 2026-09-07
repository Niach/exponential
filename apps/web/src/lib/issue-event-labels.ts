// Pure, hook-free phrase helpers for issue_events rows (EXP-759). Shared by
// the product timeline (components/comment-rows/event.tsx) and the admin team
// activity log, so a status change reads the same on both.

// EXP-314: `status_changed` payloads carry the human status NAMES
// (`fromName`/`toName`) alongside the legacy enum anchors, so a custom status
// reads as itself. Rows written before EXP-314 have no names — fall back to
// the enum munge. Retired enum tokens keep their historic label (EXP-685:
// `todo` is gone from the vocabulary, but old events still name it); iOS
// EventPhrases, Android labelFor and desktop timeline.rs mirror this map.
export const RETIRED_STATUS_LABELS: Record<string, string> = { todo: `Todo` }

export function statusLabel(
  payload: Record<string, unknown>,
  side: `to` | `from`
): string {
  const name = payload[side === `to` ? `toName` : `fromName`]
  if (typeof name === `string` && name.length > 0) return name
  const token = String(payload[side] ?? ``)
  return RETIRED_STATUS_LABELS[token] ?? token.replace(/_/g, ` `)
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
  payload: Record<string, unknown> | null
): string {
  const p = payload ?? {}
  switch (type) {
    case `created`:
      return `created`
    case `status_changed`: {
      const to = statusLabel(p, `to`)
      return to ? `changed status to ${to}` : `changed status`
    }
    case `assignee_changed`:
      return p.to ? `changed the assignee` : `removed the assignee`
    case `label_added`:
      return `added a label`
    case `label_removed`:
      return `removed a label`
    case `priority_changed`:
      return `set priority to ${priorityLabel(p.to)}`
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

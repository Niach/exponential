import { SEMANTIC_ICONS, type IconConcept } from "@exp/icons"
import type { NotificationRow } from "./model"

// EXP-1183 — the inbox's pure half (`components/inbox/inbox-view.tsx`):
// the per-type glyph concept and the grouping — an issue's notifications
// fold into ONE row (its newest leads), issue-less ones (`agent_message`,
// `session_blocked`) stay a row each.

/** EXP-273: `notification_type` maps 1:1 onto `notification-<kebab>`; a type
 *  this build has no concept for falls back to the bell the web draws. */
export function notificationConcept(type: string): IconConcept {
  const concept = `notification-${type.replace(/_/g, `-`)}`
  return concept in SEMANTIC_ICONS ? (concept as IconConcept) : `nav-inbox`
}

export interface InboxGroup {
  key: string
  /** Newest first — `items[0]` is what the row shows. */
  items: NotificationRow[]
  unread: number
}

/** Rows in the tool's order (newest first); an issue's group sits where its
 *  newest notification does. */
export function inboxGroups(rows: readonly NotificationRow[]): InboxGroup[] {
  const groups: InboxGroup[] = []
  const byIssue = new Map<string, InboxGroup>()
  for (const row of rows) {
    const unread = row.readAt === null || row.readAt === undefined ? 1 : 0
    const existing = row.issueId ? byIssue.get(row.issueId) : undefined
    if (existing) {
      existing.items.push(row)
      existing.unread += unread
      continue
    }
    const group: InboxGroup = {
      key: row.issueId ? `issue:${row.issueId}` : `message:${row.id}`,
      items: [row],
      unread,
    }
    groups.push(group)
    if (row.issueId) byIssue.set(row.issueId, group)
  }
  return groups
}

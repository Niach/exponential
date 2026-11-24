// GENERATED from packages/plugin-kit/src/model/core/inbox.ts (bun run plugin-kit:generate). Do not edit.
import type { NotificationRow } from "./wire"
import { clip } from "./issue"

// Which inbox rows interrupt a person at work, and what the interruption
// says: the row's own title and body, the words the inbox shows.

/** A comment, a mention, a reporter's reply, a session that waits. */
export const TOAST_NOTIFICATION_TYPES: readonly string[] = [`issue_comment`, `issue_mention`, `reporter_reply`, `session_blocked`]

export interface SeenMark {
  id: string
  createdAt: string
}

/** The newest row's mark (rows arrive newest first). */
export function newestMark(rows: readonly NotificationRow[]): SeenMark | null {
  const first = rows[0]
  return first ? { id: first.id, createdAt: first.createdAt } : null
}

/** The unread rows that arrived after `seen` and are worth a toast, oldest
 *  first; `issueId` narrows to one issue (session rows carry none and always
 *  pass). No mark yet = nothing: the first poll only sets it. */
export function freshNotifications(
  rows: readonly NotificationRow[],
  seen: SeenMark | null | undefined,
  issueId?: string | null
): NotificationRow[] {
  if (!seen) return []
  return rows
    .filter((row) => row.readAt == null)
    .filter((row) => row.id !== seen.id && row.createdAt > seen.createdAt)
    .filter((row) => TOAST_NOTIFICATION_TYPES.includes(row.type))
    .filter((row) => !issueId || !row.issueId || row.issueId === issueId)
    .reverse()
}

/** `Mia commented on EXP-12 · "Looks good, one nit…"`. */
export function notificationToast(row: NotificationRow, columns = 120): string {
  const body = row.body?.trim()
  return clip(body ? `${row.title} · ${body}` : row.title, columns)
}

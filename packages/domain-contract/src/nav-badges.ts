import fixture from "../fixtures/nav-badges.json" with { type: "json" }
import type { ChatsListGroup, TreeSessionRead } from "./session-tree"

// #FEED-232 — ONE count badge ×4: every nav destination says HOW MANY things
// in it need the viewer, never a dot. The spec is nav-badges.json's
// `$comment`; this file is its TS reference (every mirror replays the cases).

export type CountBadgeTone = `attention` | `muted`

/** A destination's badge: `count` ≤ 0 = no badge. */
export interface CountBadgeValue {
  count: number
  tone: CountBadgeTone
}

export const COUNT_BADGE_MAX: number = fixture.max

const NONE: CountBadgeValue = { count: 0, tone: `muted` }

/** What the badge prints: `''` (nothing renders) at zero or less, `99+`
 *  past the cap (`labelCases`). */
export function countBadgeLabel(count: number, max: number = COUNT_BADGE_MAX): string {
  if (count <= 0) return ``
  return count > max ? `${max}+` : String(count)
}

/** The Chats destination (`chatsCases`): distinct depth-0 chats that need
 *  the viewer or hold an unread reply; an archived root counts only while
 *  it needs the viewer. Attention when one of them needs the viewer. */
export function chatsBadge(groups: readonly ChatsListGroup[], reads: readonly TreeSessionRead[] = []): CountBadgeValue {
  const archived = new Set(reads.filter((r) => r.archivedAt).map((r) => r.sessionId))
  const counted = new Set<string>()
  let attention = false
  for (const group of groups) {
    for (const row of group.rows) {
      if (row.depth !== 0) continue
      const lit = row.needsYou || (row.unread && !archived.has(row.sessionId))
      if (!lit) continue
      counted.add(row.sessionId)
      if (row.needsYou) attention = true
    }
  }
  if (counted.size === 0) return NONE
  return { count: counted.size, tone: attention ? `attention` : `muted` }
}

/** An agent row, an app row or a chat-list row (`rowCases`): its unread
 *  agent messages, at least 1 while it waits on the viewer. */
export function rowBadge(row: { unreadCount: number; needsYou: boolean }): CountBadgeValue {
  const unread = Math.max(0, row.unreadCount)
  if (row.needsYou) return { count: Math.max(1, unread), tone: `attention` }
  return unread > 0 ? { count: unread, tone: `muted` } : NONE
}

/** The OS app icon (`appIconCases`): every team's needs-you rows summed. */
export function appIconBadge(teams: readonly { needsYou: number }[]): number {
  return teams.reduce((sum, t) => sum + Math.max(0, t.needsYou), 0)
}

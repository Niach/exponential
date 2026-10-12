import { HOME_RECENT } from "@exp/domain-contract/home-greeting"
import type { CodingSession } from "@/db/schema"

// EXP-1253 (EXP-1259 D1): the home column's RECENT band (composer-home.json
// `recent`): the caller's last `recent.count` OWN top-level runs in the
// active team (no parent run), any status, newest first by `updatedAt`, then
// `createdAt`. Pure, so the rule is a test.

export type HomeRecentSession = Pick<
  CodingSession,
  `id` | `userId` | `teamId` | `parentSessionId` | `updatedAt` | `createdAt`
>

const time = (value: Date | string | null | undefined): number =>
  value ? new Date(value).getTime() : 0

export function selectHomeRecent<T extends HomeRecentSession>(
  sessions: readonly T[],
  currentUserId: string | undefined,
  teamId: string | undefined,
  count: number = HOME_RECENT.count
): T[] {
  if (!currentUserId || !teamId) return []
  return sessions
    .filter(
      (session) =>
        session.userId === currentUserId &&
        session.teamId === teamId &&
        session.parentSessionId == null
    )
    .sort(
      (a, b) =>
        time(b.updatedAt) - time(a.updatedAt) || time(b.createdAt) - time(a.createdAt)
    )
    .slice(0, count)
}

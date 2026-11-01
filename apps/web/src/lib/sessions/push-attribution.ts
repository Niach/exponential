import { and, eq, gte, inArray, isNotNull, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { repositories, sessions } from "@/db/schema"
import { soleHeadSession, type GuideTreeRow } from "@/lib/sessions/guide-entries"

/** #EXP-1540: how long after it ended a session may still be credited with
 *  a push (it pushed, then ended before GitHub delivered the webhook). */
export const PUSH_GRACE_MS = 2 * 60 * 1000

/** A session on the pushed branch, in the pushed repository. */
export interface PushCandidate {
  id: string
  status: string
  busy: boolean
  background: boolean
  endedAt: Date | null
}

/** #EXP-1540: the session a branch push is credited to. The device pushes
 *  with the clone's shared App token, so the delivery names no session: the
 *  pusher is the one session working on that branch at the time. One
 *  candidate = it; several = the only one mid-turn (busy, background work,
 *  or just ended); still ambiguous = nobody (never "the newest"). */
export function pushAuthor(candidates: readonly PushCandidate[], now: Date): string | null {
  const live = candidates.filter((row) => row.status !== `ended`)
  const fresh = (row: PushCandidate) =>
    row.status === `ended` && !!row.endedAt && now.getTime() - row.endedAt.getTime() <= PUSH_GRACE_MS
  const working = candidates.filter((row) => (row.status !== `ended` && (row.busy || row.background)) || fresh(row))
  if (working.length === 1) return working[0]!.id
  if (working.length === 0 && live.length === 1) return live[0]!.id
  return null
}

/** #EXP-1540: who a PR with no recorded session resolves to through its
 *  head: the session credited with the latest push of that branch, else the
 *  one lineage sitting on it (`soleHeadSession`), else nobody. */
export function headSession(
  tree: readonly (GuideTreeRow & { branch?: string | null; pushedBranch?: string | null; pushedAt?: Date | null })[],
  head: string
): string | null {
  const name = head.trim()
  let pusher: { id: string; at: number } | null = null
  for (const row of tree) {
    if (!row.pushedAt || row.pushedBranch?.trim() !== name) continue
    const at = row.pushedAt.getTime()
    if (!pusher || at > pusher.at) pusher = { id: row.id, at }
  }
  return pusher?.id ?? soleHeadSession(tree, name)
}

/** #EXP-1540: credits a GitHub `push` of `branch` in `fullName` to its
 *  session (`pushAuthor`): `pushed_branch` + `pushed_at`, server-only. Returns the credited session, if any. */
export async function attributePush(args: { fullName: string; branch: string; now?: Date }): Promise<string | null> {
  const now = args.now ?? new Date()
  const candidates = await db
    .select({
      id: sessions.id,
      status: sessions.status,
      busy: sessions.busy,
      background: sessions.background,
      endedAt: sessions.endedAt,
    })
    .from(sessions)
    .where(
      and(
        eq(sessions.branch, args.branch),
        inArray(
          sessions.repositoryId,
          db.select({ id: repositories.id }).from(repositories).where(eq(repositories.fullName, args.fullName))
        ),
        or(
          inArray(sessions.status, [`running`, `idle`]),
          and(
            eq(sessions.status, `ended`),
            isNotNull(sessions.endedAt),
            gte(sessions.endedAt, new Date(now.getTime() - PUSH_GRACE_MS))
          )
        )
      )
    )
  const id = pushAuthor(candidates, now)
  if (!id) return null
  // Never the session's `updated_at` (the stuck clock, sync traffic):
  // `update_updated_at` honours the transaction-local GUC (brief.ts).
  await db.transaction(async (tx) => {
    await tx.execute(sql`SELECT set_config('exponential.preserve_timestamps', 'on', true)`)
    await tx.update(sessions).set({ pushedBranch: args.branch, pushedAt: now }).where(eq(sessions.id, id))
  })
  return id
}

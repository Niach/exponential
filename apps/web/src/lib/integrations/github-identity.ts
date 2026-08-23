// GitHub account ↔ app user resolution (EXP-617).
//
// The in-process PR actor claims (pr-actor-claims.ts) only ever cover pull
// requests OUR SERVER created or merged. Everything a human does on github.com
// itself — opening a PR from the compare view, hitting Merge, an agent running
// `gh pr create` under the developer's own credentials — arrives as a webhook
// with a GitHub identity attached and nothing we could match it against. This
// module is that missing half, and the two are exactly complementary: our own
// PR calls go out on an INSTALLATION token, so their `sender` is the App bot
// and never resolves here; a human's own action carries their real account.
//
// Matching is on GitHub's NUMERIC account id whenever the payload has one.
// Logins are renameable AND re-registerable, so a login-keyed lookup can hand
// the notification-suppression decision to whoever squatted a freed name.

import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { githubUserIdentities } from "@/db/schema"

// Shaped like GitHub's account object so webhook and REST payloads can be
// handed over verbatim.
export interface GithubActorRef {
  id?: number | null
  login?: string | null
  // GitHub's `type` discriminator on the account object ("User" | "Bot" | …).
  type?: string | null
}

// Our own App's PR opens and merges are attributed to `exponential[bot]`.
// Resolving or excluding a bot would be wrong twice over: it maps to nobody,
// and if it ever did map it would print "exponential[bot] opened a pull
// request". Checked before any query.
export function isBotActor(actor: GithubActorRef): boolean {
  if (actor.type === `Bot`) return true
  const login = actor.login?.trim().toLowerCase()
  return Boolean(login && login.endsWith(`[bot]`))
}

/**
 * The app user behind a GitHub actor, or null. Never throws — a lookup failure
 * degrades to the pre-EXP-617 attribution ladder, never to a lost or misrouted
 * notification.
 */
export async function resolveAppUserForGithubActor(
  actor: GithubActorRef | null | undefined
): Promise<string | null> {
  try {
    if (!actor) return null
    if (isBotActor(actor)) return null

    if (actor.id != null) {
      const [row] = await db
        .select({ userId: githubUserIdentities.userId })
        .from(githubUserIdentities)
        .where(eq(githubUserIdentities.githubUserId, actor.id))
        .limit(1)
      // NO login fallback here on purpose: an id we have never seen means this
      // account is not mapped, full stop. Falling back to the login would
      // resolve a RENAMED-AWAY row belonging to a different human.
      return row?.userId ?? null
    }

    const login = actor.login?.trim()
    if (!login) return null
    // limit(2) so an ambiguous login (two rows, one of them stale after a
    // rename + squat) resolves to nobody rather than to a guess.
    const rows = await db
      .select({ userId: githubUserIdentities.userId })
      .from(githubUserIdentities)
      .where(
        sql`lower(${githubUserIdentities.githubLogin}) = ${login.toLowerCase()}`
      )
      .limit(2)
    return rows.length === 1 ? rows[0].userId : null
  } catch (err) {
    console.error(`[github-identity] resolve failed:`, err)
    return null
  }
}

/**
 * Record the GitHub account a user just proved control of (the connect OAuth
 * callback's code exchange). Best-effort: an identity write must never break
 * the user-visible connect flow.
 *
 * The conflict target is the GitHub id, and it re-points `user_id` — the same
 * human reconnecting under a different app account MOVES their mapping rather
 * than hitting the unique index.
 */
export async function recordGithubIdentity(args: {
  userId: string
  githubUserId: number
  githubLogin: string
}): Promise<void> {
  try {
    const now = new Date()
    await db
      .insert(githubUserIdentities)
      .values({
        userId: args.userId,
        githubUserId: args.githubUserId,
        githubLogin: args.githubLogin,
        verifiedAt: now,
      })
      .onConflictDoUpdate({
        target: githubUserIdentities.githubUserId,
        set: {
          userId: args.userId,
          githubLogin: args.githubLogin,
          verifiedAt: now,
          updatedAt: now,
        },
      })
  } catch (err) {
    console.error(`[github-identity] record failed:`, err)
  }
}

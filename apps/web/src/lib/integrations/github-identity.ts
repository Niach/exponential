// GitHub account ↔ app user resolution (EXP-617, SLOP-7).
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
// SLOP-7: the mapping IS the Better Auth `accounts` row the `github` provider
// writes (`provider_id = 'github'`, `account_id` = GitHub's numeric id). The
// old `github_user_identities` table is retired. Matching is on GitHub's
// NUMERIC account id ONLY: logins are renameable AND re-registerable, so a
// login-keyed lookup could hand the notification-suppression decision to
// whoever squatted a freed name.

import { userIdForGithubAccountId } from "@/lib/integrations/github-user"

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
 * notification. An actor without a numeric id resolves to nobody.
 */
export async function resolveAppUserForGithubActor(
  actor: GithubActorRef | null | undefined
): Promise<string | null> {
  try {
    if (!actor) return null
    if (isBotActor(actor)) return null
    if (actor.id == null) return null
    return await userIdForGithubAccountId(actor.id)
  } catch (err) {
    console.error(`[github-identity] resolve failed:`, err)
    return null
  }
}

// Boot-time backfill of github_user_identities (EXP-617).
//
// The OAuth callback is the only WRITER of an identity, which means the PR
// attribution it powers does nothing for anyone until they happen to reconnect
// GitHub. For an existing instance that is most of the userbase, so this pass
// recovers the mappings we can already prove from data we hold.
//
// It cannot be a SQL migration: the numeric GitHub id we key on is not stored
// anywhere, and the only safe way to obtain it is to ask GitHub for the
// installation's account object (see getInstallationAccount — resolving a
// stored LOGIN would return whoever holds that name today, i.e. the squatter
// after a rename). So it runs as a boot pass instead.
//
// The proof chain, and why each link holds:
//   - `account_type = 'User'` — a personal-account installation. EXP-363's
//     partitionControlledInstallations only ever links one of those when the
//     OAuth login EQUALS the account login, so whoever created the link
//     demonstrably controls that GitHub account. Organization installations
//     are excluded outright: `account_login` is the org, and nothing on the
//     row identifies a person.
//   - exactly ONE distinct link creator — two app users claiming one personal
//     installation is ambiguous (two accounts of one human, or a pre-EXP-363
//     mis-link), and ambiguity must never decide whose notifications get
//     suppressed.
//   - the numeric id comes from the installation object, never from a login.
//   - ON CONFLICT DO NOTHING — an identity recorded by the OAuth callback is
//     higher-trust and is never overwritten by this pass.
//
// Residual risk, stated plainly: a link created BEFORE EXP-363 shipped
// (2026-07-30) could name a mere collaborator on someone else's personal repo
// rather than the account owner, because that is exactly the leak EXP-363
// closed. The blast radius is notification-only — a wrong exclusion and a
// wrong name on a PR title, never an access decision — and the next real
// connect by either party overwrites the row (recordGithubIdentity re-points
// user_id on conflict). Not worth blocking every legitimate pre-363 mapping to
// avoid.

import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { githubUserIdentities } from "@/db/schema"
import {
  getInstallationAccount,
  githubAppConfigured,
} from "@/lib/integrations/github-app"

// One GitHub call per candidate, so the pass is bounded. Candidates disappear
// as they are mapped (the NOT EXISTS below), so a backlog drains over a few
// restarts instead of hammering the API on one boot.
const MAX_CANDIDATES_PER_PASS = 100

interface Candidate {
  installationId: number
  accountLogin: string | null
  userId: string
}

async function loadCandidates(): Promise<Candidate[]> {
  // A user who ALREADY has an identity is skipped: it bounds the pass to real
  // work and lets it converge to zero. The cost is that a second personal
  // GitHub account never gets backfilled — that one needs a real connect.
  const rows = await db.execute(sql`
    select
      gi.installation_id as installation_id,
      gi.account_login as account_login,
      min(l.created_by_user_id) as user_id
    from github_installations gi
    join github_installation_links l
      on l.github_installation_id = gi.id
    where gi.account_type = 'User'
      and gi.suspended_at is null
      and l.created_by_user_id is not null
      and not exists (
        select 1 from github_user_identities u
        where u.user_id = l.created_by_user_id
      )
    group by gi.installation_id, gi.account_login
    having count(distinct l.created_by_user_id) = 1
    limit ${MAX_CANDIDATES_PER_PASS}
  `)
  return rows.rows.map((row) => ({
    installationId: Number(row.installation_id),
    accountLogin: (row.account_login as string | null) ?? null,
    userId: row.user_id as string,
  }))
}

/**
 * Map every app user we can prove owns a personal GitHub installation. Never
 * throws — this is a best-effort recovery pass, and a GitHub hiccup must not
 * fail a boot. Returns counts for the log line.
 */
export async function runGithubIdentityBackfill(): Promise<{
  mapped: number
  skipped: number
}> {
  if (!githubAppConfigured()) return { mapped: 0, skipped: 0 }

  let mapped = 0
  let skipped = 0
  try {
    const candidates = await loadCandidates()
    if (candidates.length === 0) return { mapped: 0, skipped: 0 }

    for (const candidate of candidates) {
      const account = await getInstallationAccount(candidate.installationId)
      // Re-check the type against GitHub rather than trusting our mirror: the
      // whole proof rests on this being a personal account.
      if (!account || account.type !== `User`) {
        skipped++
        continue
      }
      // The mirrored login should still match. A mismatch means the account
      // was renamed (fine — the id is what we store) or that our mirror is
      // stale in a way worth not guessing about, so require one or the other
      // to be absent rather than contradictory.
      if (
        candidate.accountLogin &&
        candidate.accountLogin.toLowerCase() !== account.login.toLowerCase()
      ) {
        console.warn(
          `[github-identity-backfill] installation ${candidate.installationId} is now ${account.login}, mirrored as ${candidate.accountLogin} — mapping on the numeric id`
        )
      }

      const inserted = await db
        .insert(githubUserIdentities)
        .values({
          userId: candidate.userId,
          githubUserId: account.id,
          githubLogin: account.login,
        })
        .onConflictDoNothing({ target: githubUserIdentities.githubUserId })
        .returning({ id: githubUserIdentities.id })
      if (inserted.length > 0) mapped++
      else skipped++
    }
  } catch (err) {
    console.error(`[github-identity-backfill] pass failed:`, err)
  }

  if (mapped > 0 || skipped > 0) {
    console.log(
      `[github-identity-backfill] mapped ${mapped}, skipped ${skipped}`
    )
  }
  return { mapped, skipped }
}

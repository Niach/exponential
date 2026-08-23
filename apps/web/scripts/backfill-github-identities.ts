/**
 * One-off backfill of `github_user_identities` (EXP-617).
 *
 * The OAuth callback is the only WRITER of an identity, so the PR attribution
 * it powers does nothing for anyone until they happen to reconnect GitHub.
 * This recovers the mappings we can already PROVE from data we hold, so an
 * existing instance does not have to wait for every user to reconnect.
 *
 * Deliberately a script and not a migration or a boot pass:
 *   - not a migration, because the numeric GitHub id we key on is stored
 *     nowhere. The only safe way to obtain it is to ask GitHub for the
 *     installation's account object (resolving a stored LOGIN through
 *     `GET /users/{login}` returns whoever holds that name TODAY, i.e. the
 *     squatter after a rename). SQL cannot make that call.
 *   - not a boot pass, because it is a handful of rows once, not a recurring
 *     reconciliation. Run it by hand after the deploy that creates the table.
 *
 * The proof chain, each link doing real work:
 *   - `account_type = 'User'` — a personal-account installation. EXP-363's
 *     partitionControlledInstallations only ever links one of those when the
 *     OAuth login EQUALS the account login, so whoever created the link
 *     demonstrably controls that GitHub account. Organization installations
 *     are excluded outright: `account_login` is the org, and nothing on the
 *     row identifies a person.
 *   - exactly ONE distinct link creator — two app users claiming one personal
 *     installation is ambiguous (two accounts of one human, or a pre-EXP-363
 *     mis-link), and ambiguity must never decide whose notifications get
 *     suppressed.
 *   - the numeric id comes from the installation object, never from a login.
 *   - ON CONFLICT DO NOTHING — an identity recorded by the OAuth callback is
 *     higher-trust and is never overwritten by this script.
 *
 * Residual risk, stated plainly: a link created BEFORE EXP-363 shipped
 * (2026-07-30) could name a mere collaborator on someone else's personal repo
 * rather than the account owner, because that is exactly the leak EXP-363
 * closed. The blast radius is notification-only — a wrong exclusion and a
 * wrong name on a PR title, never an access decision — and the next real
 * connect by either party overwrites the row (recordGithubIdentity re-points
 * user_id on conflict). `--dry-run` prints the link's age so a small prod set
 * can be eyeballed before applying.
 *
 * Usage (from apps/web, with the prod DATABASE_URL + the same GitHub App env
 * the web app uses — GITHUB_APP_ID / GITHUB_APP_PRIVATE_KEY):
 *   bun run backfill:github-identities -- --dry-run   # report only, no writes
 *   bun run backfill:github-identities                # apply
 */
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { githubUserIdentities } from "@/db/schema"
import {
  getInstallationAccount,
  githubAppConfigured,
} from "@/lib/integrations/github-app"

export interface IdentityCandidate {
  installationId: number
  accountLogin: string | null
  userId: string
  userEmail: string | null
  linkedAt: Date | null
}

/**
 * Every app user we can prove owns a personal GitHub installation and who has
 * no identity row yet. Exported so the test can drive it and so the same query
 * can be run read-only against prod before applying anything.
 */
export async function loadIdentityCandidates(): Promise<IdentityCandidate[]> {
  // A user who ALREADY has an identity is skipped — the OAuth callback's row
  // is higher-trust. The cost is that a second personal GitHub account is
  // never backfilled; that one needs a real connect.
  const rows = await db.execute(sql`
    select
      gi.installation_id as installation_id,
      gi.account_login as account_login,
      min(l.created_by_user_id) as user_id,
      min(u.email) as user_email,
      min(l.created_at) as linked_at
    from github_installations gi
    join github_installation_links l
      on l.github_installation_id = gi.id
    join users u
      on u.id = l.created_by_user_id
    where gi.account_type = 'User'
      and gi.suspended_at is null
      and l.created_by_user_id is not null
      and not exists (
        select 1 from github_user_identities gu
        where gu.user_id = l.created_by_user_id
      )
    group by gi.installation_id, gi.account_login
    having count(distinct l.created_by_user_id) = 1
    order by min(l.created_at)
  `)
  return rows.rows.map((row) => ({
    installationId: Number(row.installation_id),
    accountLogin: (row.account_login as string | null) ?? null,
    userId: row.user_id as string,
    userEmail: (row.user_email as string | null) ?? null,
    linkedAt: row.linked_at ? new Date(row.linked_at as string) : null,
  }))
}

// EXP-363 shipped 2026-07-30. Links older than that predate the ownership
// gate, so they are the ones worth a human glance before applying.
const EXP_363_SHIPPED = new Date(`2026-07-30T00:00:00Z`)

export async function runGithubIdentityBackfill(opts: {
  dryRun: boolean
}): Promise<{ mapped: number; skipped: number }> {
  const candidates = await loadIdentityCandidates()
  if (candidates.length === 0) {
    console.log(`[backfill] no candidates — nothing to map.`)
    return { mapped: 0, skipped: 0 }
  }
  console.log(`[backfill] ${candidates.length} candidate installation(s)`)

  let mapped = 0
  let skipped = 0
  for (const candidate of candidates) {
    const account = await getInstallationAccount(candidate.installationId)
    // Re-check the type against GitHub rather than trusting our mirror: the
    // whole proof rests on this being a personal account.
    if (!account || account.type !== `User`) {
      console.warn(
        `[backfill] SKIP installation ${candidate.installationId} (${candidate.accountLogin ?? `?`}) — ${account ? `account is a ${account.type}` : `could not read the installation`}`
      )
      skipped++
      continue
    }
    const preGate =
      candidate.linkedAt && candidate.linkedAt < EXP_363_SHIPPED
        ? ` [PRE-EXP-363 LINK — verify this is really their account]`
        : ``
    const renamed =
      candidate.accountLogin &&
      candidate.accountLogin.toLowerCase() !== account.login.toLowerCase()
        ? ` [renamed from ${candidate.accountLogin}]`
        : ``
    console.log(
      `[backfill] ${candidate.userEmail ?? candidate.userId} -> github ${account.login} (id ${account.id})${renamed}${preGate}`
    )

    if (opts.dryRun) continue

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
    else {
      console.warn(
        `[backfill] SKIP github id ${account.id} — already mapped to another user`
      )
      skipped++
    }
  }

  console.log(
    opts.dryRun
      ? `[backfill] dry run: ${candidates.length} would be attempted, ${skipped} would be skipped. Re-run without --dry-run to apply.`
      : `[backfill] mapped ${mapped}, skipped ${skipped}`
  )
  return { mapped, skipped }
}

async function main() {
  const dryRun =
    process.argv.includes(`--dry-run`) || process.argv.includes(`-n`)

  if (!githubAppConfigured()) {
    console.error(
      `[backfill] GitHub App is not configured (GITHUB_APP_ID / GITHUB_APP_PRIVATE_KEY). Nothing to do.`
    )
    process.exit(1)
  }

  await runGithubIdentityBackfill({ dryRun })
  process.exit(0)
}

// Importable by the test without running; executed when invoked directly.
if (import.meta.main) {
  void main()
}

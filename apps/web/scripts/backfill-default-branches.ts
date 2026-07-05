/**
 * One-off backfill: reconcile `repositories.default_branch` with GitHub's live
 * value (L30 — the default branch is resolved, never assumed).
 *
 * Historically `connectRepositoryInTx` blind-seeded `main`, so any repo whose
 * real default is `master` (or anything else) has a wrong row that breaks the
 * desktop launcher's `git worktree add … origin/<default>`. This script walks
 * every non-archived repository, asks the GitHub App for the authoritative
 * default branch, and fixes disagreements.
 *
 * Safe to run repeatedly (it only writes rows that disagree). Requires the same
 * GitHub App env the web app uses (GITHUB_APP_ID / GITHUB_APP_PRIVATE_KEY / …).
 *
 * Usage (from apps/web):
 *   bun run backfill:default-branches            # apply fixes
 *   bun run backfill:default-branches -- --dry-run   # report only, no writes
 */
import { eq, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { repositories } from "@/db/schema"
import {
  githubAppConfigured,
  resolveRepoDefaultBranch,
} from "@/lib/integrations/github-app"

async function main() {
  const dryRun =
    process.argv.includes(`--dry-run`) || process.argv.includes(`-n`)

  if (!githubAppConfigured()) {
    console.error(
      `[backfill] GitHub App is not configured (GITHUB_APP_ID / GITHUB_APP_PRIVATE_KEY). Nothing to do.`
    )
    process.exit(1)
  }

  const rows = await db
    .select({
      id: repositories.id,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
    })
    .from(repositories)
    .where(isNull(repositories.archivedAt))

  console.log(
    `[backfill] ${rows.length} active repositor${rows.length === 1 ? `y` : `ies`} to check${dryRun ? ` (dry-run)` : ``}`
  )

  let fixed = 0
  let unresolved = 0
  let ok = 0

  for (const repo of rows) {
    let live: string | null = null
    try {
      live = await resolveRepoDefaultBranch(repo.fullName)
    } catch (err) {
      console.warn(`[backfill] ${repo.fullName}: lookup threw — skipping`, err)
      unresolved++
      continue
    }
    if (!live) {
      console.warn(
        `[backfill] ${repo.fullName}: could not resolve live default (App not installed / repo gone) — leaving "${repo.defaultBranch}"`
      )
      unresolved++
      continue
    }
    if (live === repo.defaultBranch) {
      ok++
      continue
    }
    console.log(
      `[backfill] ${repo.fullName}: "${repo.defaultBranch}" → "${live}"${dryRun ? ` (dry-run, not written)` : ``}`
    )
    if (!dryRun) {
      await db
        .update(repositories)
        .set({ defaultBranch: live })
        .where(eq(repositories.id, repo.id))
    }
    fixed++
  }

  console.log(
    `[backfill] done — ${fixed} ${dryRun ? `would be fixed` : `fixed`}, ${ok} already correct, ${unresolved} unresolved`
  )
  process.exit(0)
}

main().catch((err) => {
  console.error(`[backfill] fatal`, err)
  process.exit(1)
})

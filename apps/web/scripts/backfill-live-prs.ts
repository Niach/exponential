/**
 * One-off (EXP-1705, merge train part 0): re-read every live pull_requests
 * row against the forge and apply the merges/closes Exponential missed, then
 * record the `pr_merged` events missing on issues whose merged PR had lost
 * its link. Run it BEFORE the merge train exists (stale "open" rows would be
 * queued).
 *
 * Usage (from apps/web, with the target's DATABASE_URL + GitHub App env):
 *   bun scripts/backfill-live-prs.ts                       # dry run
 *   bun scripts/backfill-live-prs.ts --apply [--move EXP-1,EXP-2] [--exclude EXP-3]
 * `--move` = the orphaned-merge issues that also take the merge automation
 * (default: none; every orphan gets its event either way). `--exclude` =
 * issues left out of the orphaned-merge part entirely (no event).
 */
import {
  applyOrphanedMerges,
  findOrphanedMerges,
  runLivePrBackfill,
} from "@/lib/integrations/pr-backfill"

function flag(name: string): string | null {
  const at = process.argv.indexOf(name)
  return at >= 0 ? (process.argv[at + 1] ?? null) : null
}

async function main() {
  const apply = process.argv.includes(`--apply`)
  const move = new Set((flag(`--move`) ?? ``).split(`,`).map((id) => id.trim()).filter(Boolean))
  const exclude = new Set((flag(`--exclude`) ?? ``).split(`,`).map((id) => id.trim()).filter(Boolean))
  const mode = apply ? `APPLY` : `dry run`

  const entries = await runLivePrBackfill({ apply })
  const acted = entries.filter((e) => e.action !== `none`)
  console.log(`[backfill] live: ${entries.length} row(s), ${acted.length} to act on (${mode})`)
  for (const e of acted) {
    const when = e.mergedAt ? ` at ${e.mergedAt.toISOString()}` : ``
    console.log(`${e.action}\t${e.url}\tlocal ${e.localState} → forge ${e.forge}${when}${e.error ? `\t${e.error}` : ``}`)
    for (const i of e.issues) {
      console.log(`\t${i.identifier}: ${i.from} → ${i.to ?? `(automation off)`}${i.after ? ` [now ${i.after}]` : ``}`)
    }
  }

  const orphans = (await findOrphanedMerges()).filter((o) => !exclude.has(o.identifier))
  const prs = new Set(orphans.map((o) => o.pullRequestId)).size
  const issueIds = new Set(orphans.map((o) => o.identifier))
  console.log(`[backfill] orphaned merges: ${orphans.length} missing pr_merged event(s), ${prs} PR(s), ${issueIds.size} issue(s) (${mode})`)
  for (const o of orphans.filter((o) => !o.settled)) {
    console.log(`\tunsettled ${o.identifier} (${o.status}) ← #${o.number} merged ${o.mergedAt?.toISOString() ?? `?`}${move.has(o.identifier) ? ` [move]` : ``}`)
  }
  if (apply) {
    const moved = await applyOrphanedMerges(orphans, move)
    console.log(`[backfill] recorded ${orphans.length} pr_merged event(s); moved: ${moved.join(`, `) || `none`}`)
    // applyPullRequestMerged's notifications and head-branch cleanup are
    // fire-and-forget: give them time before the process exits.
    await new Promise((resolve) => setTimeout(resolve, 15_000))
  }
  process.exit(entries.some((e) => e.action === `failed`) ? 1 : 0)
}

void main()

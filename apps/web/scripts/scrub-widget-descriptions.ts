/**
 * One-off privacy backfill (EXP-42b): pre-EXP-42b widget submissions appended
 * a "Reported via widget" metadata block — reporter name/email, page URL,
 * viewport/user agent, custom data — to `issues.description`, and public
 * feedback boards render descriptions to ANONYMOUS visitors. This script
 * strips that legacy block from every issue that has a `widget_submissions`
 * row; the structured copy already lives server-only in `widget_submissions`
 * (members keep it via `widgets.submissionForIssue`), so nothing is lost.
 *
 * Safe to run repeatedly (descriptions without the marker are left untouched).
 * Run against prod/staging as part of the EXP-42b deploy — until then the
 * historical PII stays publicly readable on live feedback boards.
 *
 * Usage (from apps/web):
 *   bun run backfill:widget-descriptions              # apply
 *   bun run backfill:widget-descriptions -- --dry-run # report only, no writes
 */
import { eq, isNotNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues, widgetSubmissions } from "@/db/schema"
import { stripLegacyWidgetMetadata } from "@/lib/widget/metadata"

async function main() {
  const dryRun =
    process.argv.includes(`--dry-run`) || process.argv.includes(`-n`)

  const rows = await db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      description: issues.description,
    })
    .from(widgetSubmissions)
    .innerJoin(issues, eq(issues.id, widgetSubmissions.issueId))
    .where(isNotNull(issues.description))

  // One submission per issue in practice, but dedupe defensively.
  const byIssue = new Map(rows.map((row) => [row.id, row]))
  console.log(
    `[scrub] ${byIssue.size} widget issue${byIssue.size === 1 ? `` : `s`} with a description to check${dryRun ? ` (dry-run)` : ``}`
  )

  let scrubbed = 0
  let clean = 0

  for (const issue of byIssue.values()) {
    const stripped = stripLegacyWidgetMetadata(issue.description ?? ``)
    if (stripped === null) {
      clean++
      continue
    }
    console.log(
      `[scrub] ${issue.identifier}: removing legacy metadata block${stripped ? `` : ` (description becomes empty)`}${dryRun ? ` (dry-run, not written)` : ``}`
    )
    if (!dryRun) {
      await db
        .update(issues)
        // Empty after the strip (metadata-only description) — store null,
        // like buildWidgetDescription callers do.
        .set({ description: stripped || null })
        .where(eq(issues.id, issue.id))
    }
    scrubbed++
  }

  console.log(
    `[scrub] done — ${scrubbed} ${dryRun ? `would be scrubbed` : `scrubbed`}, ${clean} already clean`
  )
  process.exit(0)
}

main().catch((err) => {
  console.error(`[scrub] fatal`, err)
  process.exit(1)
})

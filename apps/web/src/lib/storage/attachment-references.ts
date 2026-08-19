import { and, eq, ilike, or } from "drizzle-orm"
import { comments, issues } from "@/db/schema"
import { replaceAttachmentReferencesWithPlaceholder } from "@/lib/storage/issue-attachments"
import { escapeLikePattern } from "@/lib/like-pattern"

type Tx = Parameters<
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

/**
 * Rewrites every markdown image in this team's issue descriptions and comment
 * bodies that points at one of `targets` to the plain-text deleted
 * placeholder, in the CALLER's transaction.
 *
 * EVERY hard delete of an attachment row must run this first — a body left
 * pointing at a dead attachment is exactly what the issues.update round-trip
 * guard rejects ("Issue descriptions can only reference images uploaded to
 * this issue"), so the description becomes unsavable. Callers: attachments.delete
 * and both comment-attachment hard-delete paths (EXP-554), whose rows may also
 * be embedded inline in the issue description or in an old-client comment body.
 *
 * A cheap LIKE prefilter (the id strings) narrows the rows; the exact markdown
 * parser decides what actually changes.
 */
export async function replaceAttachmentReferencesInTx(
  tx: Tx,
  args: {
    // Lowercased here: the rewrite compares against lowercase ids extracted
    // from stored URLs, so a caller-supplied uppercase uuid must not miss.
    targets: { id: string; filename: string }[]
    teamId: string
    origin: string
  }
): Promise<void> {
  const targets = args.targets.map((target) => ({
    id: target.id.toLowerCase(),
    filename: target.filename,
  }))
  if (targets.length === 0) return

  const patterns = targets.map((target) => `%${escapeLikePattern(target.id)}%`)

  const [issueRows, commentRows] = await Promise.all([
    tx
      .select({ id: issues.id, description: issues.description })
      .from(issues)
      .where(
        and(
          eq(issues.teamId, args.teamId),
          or(...patterns.map((pattern) => ilike(issues.description, pattern)))
        )
      ),
    tx
      .select({ id: comments.id, body: comments.body })
      .from(comments)
      .where(
        and(
          eq(comments.teamId, args.teamId),
          or(...patterns.map((pattern) => ilike(comments.body, pattern)))
        )
      ),
  ])

  const rewriteAll = (text: string) => {
    let next = text
    let changed = false
    for (const target of targets) {
      const result = replaceAttachmentReferencesWithPlaceholder(
        next,
        target.id,
        args.origin,
        target.filename
      )
      next = result.text
      changed = changed || result.changed
    }
    return { text: next, changed }
  }

  for (const issueRow of issueRows) {
    const rewritten = rewriteAll(issueRow.description ?? ``)
    if (!rewritten.changed) continue

    // Direct tx.update rather than issues.update: this write is the one
    // legitimate way to produce a description that no longer matches the
    // attachment it used to reference. The update_updated_at trigger still
    // bumps the row so every client re-syncs the new text.
    await tx
      .update(issues)
      .set({ description: rewritten.text })
      .where(eq(issues.id, issueRow.id))
  }

  for (const commentRow of commentRows) {
    const rewritten = rewriteAll(commentRow.body)
    if (!rewritten.changed) continue

    await tx
      .update(comments)
      .set({ body: rewritten.text })
      .where(eq(comments.id, commentRow.id))
  }
}

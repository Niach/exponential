import { and, eq, ilike, isNotNull, or, sql } from "drizzle-orm"
import type { db } from "@/db/connection"
import { attachments, comments, issues } from "@/db/schema"
import { collectReferencedAttachmentIds, isAcceptedImageContentType } from "@/lib/storage/issue-attachments"

type Tx = Parameters<Parameters<(typeof db)[`transaction`]>[0]>[0]

// Read-only surface shared by the connection and a transaction handle.
type Queryable = Pick<Tx, `select` | `execute`>

const UUID = `[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}`

/**
 * #EXP-2115: which of `ids` anything OUTSIDE issue and comment markdown
 * still names: a chat message (body, card, issue brief, attachment list), a
 * Guide, a draft, a prompt, a skill, an agent, the team's own icon and
 * prompt, an app record (trashed too). Any uuid in a text that mentions
 * attachments counts: too wide keeps an image, too narrow loses one.
 *
 * SEALED content is unreadable here, so it is judged by what is clear about
 * it: an issue's image stays while a sealed tree (or a message inside one)
 * is linked to that issue, references it or was forwarded it.
 */
export async function findStoredImageReferencesInTx(
  tx: Pick<Tx, `execute`>,
  teamId: string,
  ids: string[]
): Promise<Set<string>> {
  if (ids.length === 0) return new Set()
  const candidates = sql`unnest(string_to_array(${ids.join(`,`)}, ',')::uuid[])`
  const result = await tx.execute(sql`
    WITH candidates AS (SELECT ${candidates} AS id),
    texts AS (
      SELECT concat_ws(' ', m.body, m.card::text, m.issue_brief) AS body FROM messages m
        WHERE m.team_id = ${teamId}
          AND (m.body ILIKE '%attachment%' OR m.card::text ILIKE '%attachment%' OR m.issue_brief ILIKE '%attachment%')
      UNION ALL SELECT s.guide::text FROM sessions s WHERE s.team_id = ${teamId} AND s.guide IS NOT NULL
      UNION ALL SELECT d.description FROM issue_drafts d WHERE d.team_id = ${teamId}
      UNION ALL SELECT p.body FROM prompts p WHERE p.team_id = ${teamId}
      UNION ALL SELECT k.body FROM skills k WHERE k.team_id = ${teamId}
      UNION ALL SELECT to_jsonb(ag)::text FROM agents ag WHERE ag.team_id = ${teamId}
      UNION ALL SELECT to_jsonb(t)::text FROM teams t WHERE t.id = ${teamId}
      UNION ALL SELECT r.data::text FROM app_records r WHERE r.team_id = ${teamId}
    ),
    named AS (
      SELECT DISTINCT lower(x[1])::uuid AS id
      FROM texts, LATERAL regexp_matches(texts.body, ${`(${UUID})`}, 'g') AS x
      UNION
      SELECT unnest(m.attachment_ids) FROM messages m
        WHERE m.team_id = ${teamId} AND m.attachment_ids IS NOT NULL
    ),
    sealed_issues AS (
      SELECT si.issue_id AS id FROM session_issues si
        JOIN sessions s ON s.id = si.session_id
        JOIN sessions root ON root.id = s.root_id
        WHERE si.team_id = ${teamId} AND root.seal_public_key IS NOT NULL
      UNION
      SELECT unnest(m.ref_issue_ids) FROM messages m
        JOIN sessions root ON root.id = m.root_id
        WHERE m.team_id = ${teamId} AND m.ref_issue_ids IS NOT NULL AND root.seal_public_key IS NOT NULL
    )
    SELECT c.id FROM candidates c WHERE c.id IN (SELECT id FROM named)
    UNION
    SELECT c.id FROM candidates c JOIN attachments a ON a.id = c.id
      WHERE a.issue_id IN (SELECT id FROM sealed_issues)
  `)
  return new Set((result.rows as { id: string }[]).map((row) => row.id))
}

/**
 * Every attachment id referenced by any issue description or comment body in
 * the team, in ONE pass: a cheap LIKE prefilter narrows the rows to texts that
 * mention the attachment route at all, then the exact markdown parser decides.
 */
async function collectTeamReferencedAttachmentIdsInTx(
  tx: Queryable,
  teamId: string,
  origin: string
): Promise<Set<string>> {
  const pattern = `%/api/attachments/%`

  const [descriptionRows, commentRows, ownerLinkedRows] = await Promise.all([
    tx
      .select({ text: issues.description })
      .from(issues)
      .where(
        and(eq(issues.teamId, teamId), ilike(issues.description, pattern))
      ),
    tx
      .select({ text: comments.body })
      .from(comments)
      .where(and(eq(comments.teamId, teamId), ilike(comments.body, pattern))),
    // Comment attachments (EXP-554) are linked via comment_id, never embedded
    // in markdown — without this they'd read as unreferenced sweep bait.
    // EXP-878: a DRAFT's attachments are the same case — their only body is
    // the unsaved draft description, which is not an issue or a comment. The
    // owning column IS the reference; they die with the draft. EXP-1247: a
    // FILE-path upload (`as_file`) is a Files row, never a markdown embed.
    tx
      .select({ id: attachments.id })
      .from(attachments)
      .where(
        and(
          eq(attachments.teamId, teamId),
          or(
            isNotNull(attachments.commentId),
            isNotNull(attachments.draftId),
            eq(attachments.asFile, true)
          )
        )
      ),
  ])

  const referenced = collectReferencedAttachmentIds(
    [...descriptionRows, ...commentRows].map((row) => row.text ?? ``),
    origin
  )
  for (const row of ownerLinkedRows) {
    referenced.add(row.id)
  }
  return referenced
}

interface SweepColumns {
  id: string
  issueId: string | null
  boardId: string | null
  commentId: string | null
  draftId: string | null
  messageId: string | null
  appId: string | null
  sessionId: string | null
  asFile: boolean
  contentType: string
}

/**
 * #EXP-2115: the ONLY rows the sweep may ever take: an inline image of an
 * issue and nothing else. A result picture, a chat image, the team icon, an
 * agent's clip, an app's record file or an upload on its way to one of them
 * is never an issue's plain inline image; its own owner reclaims it
 * (lib/session-attachment-sweep.ts).
 */
export function isSweepableImage(row: SweepColumns): boolean {
  return (
    row.issueId !== null &&
    row.boardId !== null &&
    !row.commentId &&
    !row.draftId &&
    !row.messageId &&
    !row.appId &&
    !row.sessionId &&
    !row.asFile &&
    isAcceptedImageContentType(row.contentType)
  )
}

export const SWEEP_COLUMNS = {
  id: attachments.id,
  issueId: attachments.issueId,
  boardId: attachments.boardId,
  commentId: attachments.commentId,
  draftId: attachments.draftId,
  messageId: attachments.messageId,
  appId: attachments.appId,
  sessionId: attachments.sessionId,
  asFile: attachments.asFile,
  contentType: attachments.contentType,
}

/** The ids the sweep would delete (the grace window aside): the list's
 *  `referenced` flag and the mutation read this one function. */
export async function collectSweepableIdsInTx(
  tx: Queryable,
  teamId: string,
  origin: string,
  rows: SweepColumns[]
): Promise<{ referencedIds: Set<string>; sweepableIds: Set<string> }> {
  const referencedIds = await collectTeamReferencedAttachmentIdsInTx(tx, teamId, origin)
  const candidates = rows.filter((row) => isSweepableImage(row) && !referencedIds.has(row.id))
  // Only for what no issue text vouches for: chats, results, icons, records.
  for (const id of await findStoredImageReferencesInTx(
    tx,
    teamId,
    candidates.map((row) => row.id)
  )) {
    referencedIds.add(id)
  }
  return {
    referencedIds,
    sweepableIds: new Set(candidates.map((row) => row.id).filter((id) => !referencedIds.has(id))),
  }
}

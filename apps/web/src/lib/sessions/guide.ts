import { TRPCError } from "@trpc/server"
import { eq, inArray } from "drizzle-orm"
import {
  SESSION_RESULT_CAPTION_MAX,
  SESSION_RESULT_FILE_PATH_MAX,
  SESSION_RESULT_REPORT_MAX,
  SESSION_RESULT_TEXT_MAX,
  type GuideEntry,
} from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { attachments, sessions, type Session } from "@/db/schema"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"
import {
  canonicalGithubPrUrl,
  exceedsGuideCap,
  guideSummary,
  isTextEntry,
  nextShowLabel,
  ownedBy,
  ownedBySubtree,
  removeGuideEntries,
  stampUntaggedGuideEntries,
  upsertGuidePicture,
  upsertGuideText,
  type GuideSummaryRow,
} from "@/lib/sessions/guide-entries"
import { postAgentMessage } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { deleteObject } from "@/lib/storage"
import { mintGuideUploadToken } from "@/lib/storage/guide-upload-token"
import {
  rollbackSessionImage,
  sessionAttachmentValues,
  type PreparedSessionImage,
} from "@/lib/storage/session-attachment-upload"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { syncPullRequestBodies } from "@/lib/pull-requests"

// EXP-1291: the thread's ONE Guide = the ROOT's `sessions.guide` = every PR
// body of the thread (`syncPullRequestBodies` re-renders them after each
// write). Every write is a jsonb read-modify-write under `FOR UPDATE` on the
// ROOT row, so two sessions of the tree writing at once never lose one
// another's entry. Entries are stamped with their writer (`sessionId`) + the
// write time (`at`); a writer replaces only its own, removes its subtree's
// (EXP-1351), and a remove that matches nothing fails (guide-entries).
// Pictures are owner-less team `attachments` rows a Guide entry references
// by id (the orphan sweep keeps every id some Guide names).

interface GuideLock {
  session: Session
  rootId: string
  guide: GuideEntry[] | null
}

/** Locks the session's root and reads its Guide. Inside a transaction. */
export async function lockGuide(tx: Tx, sessionId: string): Promise<GuideLock> {
  const [session] = await tx.select().from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!session) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  const [root] = await tx
    .select({ guide: sessions.guide })
    .from(sessions)
    .where(eq(sessions.id, session.rootId))
    .limit(1)
    .for(`update`)
  return { session, rootId: session.rootId, guide: (root?.guide ?? null) as GuideEntry[] | null }
}

export async function saveGuide(tx: Tx, rootId: string, guide: GuideEntry[]): Promise<void> {
  await tx.update(sessions).set({ guide, updatedAt: new Date() }).where(eq(sessions.id, rootId))
}

/** Reclaims removed/replaced pictures (rows + blobs), after commit. */
async function reclaimPictures(ids: string[]): Promise<void> {
  if (ids.length === 0) return
  const gone = await db
    .delete(attachments)
    .where(inArray(attachments.id, ids))
    .returning({ storageKey: attachments.storageKey })
  for (const row of gone) {
    await deleteObject(row.storageKey).catch((error: unknown) =>
      console.error(`[guide] failed to delete a replaced picture`, error)
    )
  }
}

function syncBodies(rootId: string) {
  void syncPullRequestBodies(rootId).catch((error: unknown) =>
    console.error(`[guide] PR body sync failed`, error)
  )
}

const guideCapError = () =>
  new TRPCError({
    code: `CONFLICT`,
    message: `This session's Guide is full (60 entries, 4000 characters of text, 120 files). Remove a topic first (remove: true).`,
  })

/** The Guide upload grant: a token-gated URL + a ready curl line. */
export function mintGuideUpload(args: {
  userId: string
  sessionId: string
  topic: string
  label: string
  show?: { attachmentId: string; caption: string | null }
}): { uploadUrl: string; curl: string; expiresAt: string } {
  const { token, expiresAt } = mintGuideUploadToken({
    sessionId: args.sessionId,
    topic: args.topic,
    label: args.label,
    userId: args.userId,
    ...(args.show ? { show: args.show } : {}),
  })
  const uploadUrl = `${appBaseUrl()}/api/guide-uploads/${token}`
  return {
    uploadUrl,
    // `--retry` (FEED-75) re-sends on the store's 503 throttling.
    curl: `curl -sS --retry 4 -F file=@screenshot.png "${uploadUrl}"`,
    expiresAt: expiresAt.toISOString(),
  }
}

/**
 * One Guide write (MCP `sessions_guide`, the Guide editor): file a topic's
 * text (+ files, + its PR), remove a picture / the text (`text: ''`) / the
 * whole topic, or with `label` mint a picture upload (the picture lands when
 * the curl runs, `/api/guide-uploads/$token`).
 */
export async function writeGuide(args: {
  userId: string
  sessionId: string
  topic: string
  text?: string | null
  files?: string[] | null
  prUrl?: string | null
  remove?: boolean
  label?: string | null
}): Promise<{
  topic: string
  uploadUrl?: string
  curl?: string
  results: GuideSummaryRow[]
}> {
  const session = await loadSessionForUser(args.userId, args.sessionId)
  await assertCanSteer(args.userId, session)
  const topic = args.topic.trim()
  const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })
  if (!topic) throw bad(`topic is required`)
  if (topic.length > SESSION_RESULT_TEXT_MAX || (args.label?.length ?? 0) > SESSION_RESULT_TEXT_MAX) {
    throw bad(`topic and label are at most ${SESSION_RESULT_TEXT_MAX} characters`)
  }
  if ((args.text?.trim().length ?? 0) > SESSION_RESULT_REPORT_MAX) {
    throw bad(`A topic's text is at most ${SESSION_RESULT_REPORT_MAX} characters: 2-3 sentences`)
  }
  if (args.files?.some((path) => path.length > SESSION_RESULT_FILE_PATH_MAX)) {
    throw bad(`File paths are at most ${SESSION_RESULT_FILE_PATH_MAX} characters`)
  }
  const owns = ownedBy(session.id)

  let prUrl: string | null | undefined = undefined
  if (args.prUrl !== undefined) {
    if (args.prUrl === null || !args.prUrl.trim()) prUrl = null
    else {
      prUrl = canonicalGithubPrUrl(args.prUrl)
      if (!prUrl) {
        throw bad(`prUrl must be a GitHub pull request URL`)
      }
    }
  }

  // A picture: mint the grant only; the route files it.
  if (args.label && !args.remove) {
    if (session.status === `ended`) {
      throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
    }
    const label = args.label.trim()
    const grant = mintGuideUpload({ userId: args.userId, sessionId: session.id, topic, label })
    const [root] = await db
      .select({ guide: sessions.guide })
      .from(sessions)
      .where(eq(sessions.id, session.rootId))
      .limit(1)
    return {
      topic,
      uploadUrl: grant.uploadUrl,
      curl: grant.curl,
      results: guideSummary(((root?.guide ?? []) as GuideEntry[]).filter(owns)),
    }
  }

  const outcome = await db.transaction(async (tx) => {
    const lock = await lockGuide(tx, session.id)
    let next: GuideEntry[]
    let removed: string[] = []
    if (args.remove) {
      const tree = await tx
        .select({ id: sessions.id, parentId: sessions.parentId })
        .from(sessions)
        .where(eq(sessions.rootId, lock.rootId))
      const target = { topic, label: args.label?.trim() || null, text: args.text === `` }
      const removal = removeGuideEntries(lock.guide, target, ownedBySubtree(session.id, tree))
      if (removal.results.length === (lock.guide ?? []).length) {
        const what = target.text ? ` text` : target.label ? ` label "${target.label}"` : ``
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Nothing in this thread's Guide matched topic "${topic}"${what} written by this session or its children`,
        })
      }
      next = removal.results
      removed = removal.removedAttachmentIds
    } else {
      const existing = (lock.guide ?? []).find(
        (row) => isTextEntry(row) && row.topic === topic && owns(row)
      )
      const text = args.text ?? existing?.text
      if (typeof text !== `string` || !text.trim()) {
        throw bad(`text is required for a new topic`)
      }
      const written = upsertGuideText(
        lock.guide,
        topic,
        text.trim(),
        args.files === null ? [] : (args.files ?? undefined),
        { at: Date.now(), sessionId: session.id, ...(prUrl !== undefined ? { prUrl } : {}) }
      )
      if (!written) throw guideCapError()
      next = written
    }
    await saveGuide(tx, lock.rootId, next)
    return { rootId: lock.rootId, next, removed }
  })

  await reclaimPictures(outcome.removed)
  syncBodies(outcome.rootId)
  return { topic, results: guideSummary(outcome.next.filter(owns)) }
}

/**
 * Files an uploaded picture (the object is already put): the owner-less
 * attachments row + the Guide entry, under the root lock. `inline` = a
 * picture shown while working (it also rides the Guide's Earlier fold).
 * Every picture posts a `picture` card on the session.
 */
export async function publishGuidePicture(
  input: {
    sessionId: string
    teamId: string
    topic: string
    /** null = the topic's next `Shot N`. */
    label: string | null
    uploaderId: string
    inline?: { caption: string | null }
  },
  prepared: PreparedSessionImage
): Promise<{ label: string; results: GuideSummaryRow[]; replaced: boolean }> {
  let displaced: string | null = null
  let label = input.label ?? ``
  let results: GuideEntry[] = []
  let keyOwnedByAnotherRow = false
  let rootId = ``
  try {
    await db.transaction(async (tx) => {
      const lock = await lockGuide(tx, input.sessionId)
      rootId = lock.rootId
      const owns = ownedBy(lock.session.id)
      label = input.label || nextShowLabel(lock.guide, input.topic)
      const caption = input.inline?.caption?.slice(0, SESSION_RESULT_CAPTION_MAX) ?? null
      const upsert = upsertGuidePicture(
        lock.guide,
        {
          topic: input.topic,
          label,
          attachmentId: prepared.attachmentId,
          width: prepared.width,
          height: prepared.height,
          ...(input.inline ? { inline: true, ...(caption ? { caption } : {}) } : {}),
          at: Date.now(),
          sessionId: lock.session.id,
        },
        owns
      )
      if (exceedsGuideCap(upsert.results.filter(owns))) throw guideCapError()
      const inserted = await tx
        .insert(attachments)
        .values(sessionAttachmentValues(prepared, { teamId: input.teamId }, input.uploaderId))
        .onConflictDoNothing({ target: attachments.id })
        .returning({ id: attachments.id })
      if (inserted.length === 0) {
        keyOwnedByAnotherRow = true
        throw new TRPCError({
          code: `CONFLICT`,
          message: `This picture was already uploaded. Ask for a new upload line.`,
        })
      }
      await saveGuide(tx, lock.rootId, upsert.results)
      // Every Guide picture shows in the conversation as a card (the device
      // cannot mint them: the bytes arrive here).
      {
        await postAgentMessage(tx, lock.session, ``, {
          kind: `picture`,
          attachmentId: prepared.attachmentId,
          width: prepared.width,
          height: prepared.height,
          caption,
        })
      }
      displaced = upsert.displacedAttachmentId
      results = upsert.results.filter(owns)
    })
  } catch (error) {
    if (!keyOwnedByAnotherRow) await rollbackSessionImage(prepared.storageKey)
    throw error
  }
  if (displaced) await reclaimPictures([displaced])
  syncBodies(rootId)
  return { label, results: guideSummary(results), replaced: displaced !== null }
}

/** EXP-1251 for the PR lane: a session opening a NEW PR tags its untagged
 *  text topics with it (inside the caller's transaction). */
export async function tagGuideForPullRequest(
  tx: Tx,
  args: { sessionId: string; prUrl: string }
): Promise<void> {
  const lock = await lockGuide(tx, args.sessionId)
  const { results, changed } = stampUntaggedGuideEntries(
    lock.guide,
    args.prUrl,
    ownedBy(lock.session.id)
  )
  if (changed) await saveGuide(tx, lock.rootId, results)
}

import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray } from "drizzle-orm"
import {
  SESSION_RESULT_CAPTION_MAX,
  SESSION_RESULT_FILE_PATH_MAX,
  SESSION_RESULT_REPORT_MAX,
  SESSION_RESULT_TEXT_MAX,
  type GuideEntry,
} from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { attachments, pullRequests, sessions, type Session } from "@/db/schema"
import { LIVE_PR_STATES } from "@/lib/pr-merge-guard"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"
import {
  canonicalGithubPrUrl,
  exceedsGuideCap,
  guideSummary,
  isTextEntry,
  logicalSessionIds,
  nextShowLabel,
  ownedBy,
  ownedByAny,
  ownedBySubtree,
  type GuideEntryFilter,
  removeGuideEntries,
  stampUntaggedGuideEntries,
  upsertGuidePicture,
  upsertGuideText,
  type GuideSummaryRow,
} from "@/lib/sessions/guide-entries"
import { postAgentMessage } from "@/lib/sessions/messages"
import type { Db, Tx } from "@/lib/sessions/tx"
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
// EXP-1397: "its own" = its LOGICAL session's (itself + the ended rows it
// continues, `logicalSessionIds`); a write with no own topic of that name
// but no text either (a prUrl/files tag) and a picture reach the subtree's,
// keeping their writer; a text write never takes a child's topic over. A
// topic of a writer with an open PR is that PR's unless it says otherwise.

interface GuideReach {
  /** The writer's logical session (itself first). */
  mine: string[]
  owns: GuideEntryFilter
  /** The logical session's whole subtree (edits and removes). */
  reaches: GuideEntryFilter
}

/** EXP-1397: who the writer is in its tree. */
async function guideReach(tx: Db, session: Session): Promise<GuideReach> {
  const tree = await tx
    .select({ id: sessions.id, parentId: sessions.parentId, branch: sessions.branch, status: sessions.status })
    .from(sessions)
    .where(eq(sessions.rootId, session.rootId))
  const mine = logicalSessionIds(session.id, tree)
  return { mine, owns: ownedByAny(mine), reaches: ownedBySubtree(mine, tree) }
}

/** EXP-1397: the newest live PR the writer's logical session opened. */
async function openPullRequestUrl(tx: Tx, rootId: string, mine: string[]): Promise<string | null> {
  const [row] = await tx
    .select({ url: pullRequests.url })
    .from(pullRequests)
    .where(
      and(
        eq(pullRequests.rootSessionId, rootId),
        inArray(pullRequests.sessionId, mine),
        inArray(pullRequests.state, [...LIVE_PR_STATES])
      )
    )
    .orderBy(desc(pullRequests.createdAt))
    .limit(1)
  return row ? (canonicalGithubPrUrl(row.url) ?? row.url) : null
}
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
      // EXP-1397: the logical session's entries, as every other response.
      results: guideSummary(((root?.guide ?? []) as GuideEntry[]).filter((await guideReach(db, session)).owns)),
    }
  }

  const outcome = await db.transaction(async (tx) => {
    const lock = await lockGuide(tx, session.id)
    const reach = await guideReach(tx, session)
    const guide = lock.guide ?? []
    let next: GuideEntry[]
    let removed: string[] = []
    if (args.remove) {
      const target = { topic, label: args.label?.trim() || null, text: args.text === `` }
      const removal = removeGuideEntries(lock.guide, target, reach.reaches)
      if (removal.results.length === guide.length) {
        const what = target.text ? ` text` : target.label ? ` label "${target.label}"` : ``
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Nothing in this thread's Guide matched topic "${topic}"${what} written by this session or its children`,
        })
      }
      next = removal.results
      removed = removal.removedAttachmentIds
    } else {
      const isTopic = (row: GuideEntry) => isTextEntry(row) && row.topic === topic
      const existing = guide.find((row) => isTopic(row) && reach.owns(row))
      // EXP-1397: no own topic and no text = a tag (prUrl, files) of the
      // subtree's entries of that topic, each keeping its writer.
      const tagged =
        existing || args.text
          ? []
          : [
              ...new Set(
                guide
                  .filter((row) => isTopic(row) && reach.reaches(row) && row.sessionId)
                  .map((row) => row.sessionId!)
              ),
            ]
      if (tagged.length > 0) {
        if (prUrl === undefined && args.files === undefined) {
          throw bad(`Topic "${topic}" is a child's: pass prUrl or files to change it (text files your own topic)`)
        }
        let written: GuideEntry[] | null = guide
        for (const owner of tagged) {
          const entry = written.find((row) => isTopic(row) && row.sessionId === owner)!
          written = upsertGuideText(
            written,
            topic,
            entry.text!.trim(),
            args.files === null ? [] : args.files,
            { at: entry.at ?? Date.now(), sessionId: owner, ...(prUrl !== undefined ? { prUrl } : {}) },
            ownedBy(owner)
          )
          if (!written) throw guideCapError()
        }
        next = written
      } else {
        const text = args.text ?? existing?.text
        if (typeof text !== `string` || !text.trim()) {
          throw bad(
            `Nothing in this thread's Guide matched topic "${topic}" written by this session or its children: text is required for a new topic`
          )
        }
        // EXP-1397: a NEW topic of a writer with an open PR is that PR's; a
        // rewrite keeps the topic's tag (an explicit `prUrl: null` sticks).
        const tag =
          prUrl !== undefined
            ? prUrl
            : existing
              ? undefined
              : ((await openPullRequestUrl(tx, lock.rootId, reach.mine)) ?? undefined)
        const written = upsertGuideText(
          lock.guide,
          topic,
          text.trim(),
          args.files === null ? [] : (args.files ?? undefined),
          { at: Date.now(), sessionId: session.id, ...(tag !== undefined ? { prUrl: tag } : {}) },
          reach.owns
        )
        if (!written) throw guideCapError()
        next = written
      }
    }
    await saveGuide(tx, lock.rootId, next)
    return { rootId: lock.rootId, next, removed, owns: reach.owns }
  })

  await reclaimPictures(outcome.removed)
  syncBodies(outcome.rootId)
  return { topic, results: guideSummary(outcome.next.filter(outcome.owns)) }
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
      const reach = await guideReach(tx, lock.session)
      label = input.label || nextShowLabel(lock.guide, input.topic)
      // EXP-1397: the logical session's picture of this (topic, label), else
      // a child's (replaced in place, its writer kept), else a new own one.
      const isPair = (row: GuideEntry) =>
        !isTextEntry(row) && row?.topic === input.topic && row?.label === label
      const child = (lock.guide ?? []).find(
        (row) => isPair(row) && !reach.owns(row) && reach.reaches(row) && row.sessionId
      )
      const writer =
        child && !(lock.guide ?? []).some((row) => isPair(row) && reach.owns(row))
          ? child.sessionId!
          : lock.session.id
      const owns = writer === lock.session.id ? reach.owns : ownedBy(writer)
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
          sessionId: writer,
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
      results = upsert.results.filter(reach.owns)
    })
  } catch (error) {
    if (!keyOwnedByAnotherRow) await rollbackSessionImage(prepared.storageKey)
    throw error
  }
  if (displaced) await reclaimPictures([displaced])
  syncBodies(rootId)
  return { label, results: guideSummary(results), replaced: displaced !== null }
}

/** EXP-1397: a merged PR's logical session's untagged text topics went out
 *  with it: stamped with its URL, they leave every still-open body of the
 *  thread (a body excludes by prUrl only). A PR with no session (UI-opened,
 *  adopted) resolves through its head: the tree's newest session on it.
 *  True = the Guide moved. */
export async function tagGuideForMergedPullRequest(pr: {
  rootSessionId: string | null
  sessionId: string | null
  head: string
  url: string
}): Promise<boolean> {
  if (!pr.rootSessionId) return false
  let sessionId = pr.sessionId
  if (!sessionId) {
    const [row] = await db
      .select({ id: sessions.id })
      .from(sessions)
      .where(and(eq(sessions.rootId, pr.rootSessionId), eq(sessions.branch, pr.head)))
      .orderBy(desc(sessions.createdAt))
      .limit(1)
    sessionId = row?.id ?? null
  }
  if (!sessionId) return false
  return tagGuideForPullRequest({ sessionId, prUrl: pr.url })
}

/** EXP-1251/EXP-1397 for the PR lane: a session opening a NEW PR (or one
 *  merging) tags its logical session's untagged text topics with it. Only
 *  entries STAMPED with the logical session: a legacy unstamped one is
 *  nobody's and stays in every body. True = the Guide moved (the caller
 *  re-renders the thread's bodies). */
export async function tagGuideForPullRequest(args: { sessionId: string; prUrl: string }): Promise<boolean> {
  return db.transaction(async (tx) => {
    const lock = await lockGuide(tx, args.sessionId)
    const mine = new Set((await guideReach(tx, lock.session)).mine)
    const { results, changed } = stampUntaggedGuideEntries(
      lock.guide,
      canonicalGithubPrUrl(args.prUrl) ?? args.prUrl,
      (row) => !!row?.sessionId && mine.has(row.sessionId)
    )
    if (changed) await saveGuide(tx, lock.rootId, results)
    return changed
  })
}

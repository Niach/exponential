import { githubFetch } from "@/lib/integrations/github-client"
import { and, asc, eq, isNotNull, isNull, ne, sql } from "drizzle-orm"
import { isPrPictureCard } from "@exp/domain-contract/results-tab"
import type { MessageCard } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { issues, messages, pullRequestIssues, pullRequests, sessions } from "@/db/schema"
import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"
import { githubApiHeaders } from "@/lib/integrations/github-app"
import { appBaseUrl, buildSessionDeepLinkPath } from "@/lib/notification-email-policy"
import { logicalSessionIds, type GuideTreeRow } from "@/lib/sessions/guide-entries"
import { headPusher, headSession } from "@/lib/sessions/push-attribution"
import { inSessions, messageSessionId } from "@/lib/sessions/messages"
import { isSealedCard } from "@/lib/sessions/sealed"

// The I/O half of a PR body (`prBodyFromPictures`, lib/pr-body.ts): the
// opener's pictures, the PR's commits and the chat link.

export interface PrPicture {
  label: string
  attachmentId: string
  url: string
}

/** One card's PR pictures (label + attachment). A plaintext card is
 *  read as is (`isPrPictureCard`). EXP-1468 (sealed-content addendum 5): a
 *  SEALED card is read through its CLEAR part only: the `pictures`
 *  projection's entries that carry an attachmentId, none when the clear
 *  `result` (or an older card's `story`) is false. Captions stay sealed: a PR body is a public export and
 *  never carries sealed text (the label stands in for the caption). */
export function prPicturesOfCard(card: unknown): { label: string; attachmentId: string }[] {
  if (isSealedCard(card)) {
    const clear = card as Record<string, unknown>
    if (clear.kind !== `surface` && clear.kind !== `question` && clear.kind !== `plan`) return []
    if ((clear.result ?? clear.story) === false || !Array.isArray(clear.pictures)) return []
    return clear.pictures.flatMap((entry: unknown) => {
      const { label, attachmentId } = (entry ?? {}) as Record<string, unknown>
      return typeof label === `string` && typeof attachmentId === `string` ? [{ label, attachmentId }] : []
    })
  }
  if (!isPrPictureCard(card)) return []
  return ((card as Extract<MessageCard, { kind: `surface` }>).pictures ?? []).flatMap((picture) =>
    picture.attachmentId ? [{ label: picture.label, attachmentId: picture.attachmentId }] : []
  )
}

/** #EXP-1402: the sessions whose pictures a PR body shows: the opener's
 *  LOGICAL session (its continuation lineage) and its subtree, never a
 *  descendant's subtree that opened a PR of its own (`claimed`: that PR's
 *  body shows them, FEED-166). Null opener = the whole tree, minus the
 *  claimed subtrees. */
export function prPictureSessionIds(
  tree: readonly GuideTreeRow[],
  openerSessionId: string | null,
  claimed: ReadonlySet<string>
): Set<string> {
  const mine = openerSessionId
    ? logicalSessionIds(openerSessionId, tree)
    : tree.filter((row) => row.parentId === null).map((row) => row.id)
  const own = new Set(mine)
  const ids = new Set(mine)
  for (let grew = true; grew; ) {
    grew = false
    for (const row of tree) {
      if (!row.parentId || !ids.has(row.parentId) || ids.has(row.id)) continue
      if (claimed.has(row.id) && !own.has(row.id)) continue
      ids.add(row.id)
      grew = true
    }
  }
  return ids
}

/** The resolved pictures of the cards of a PR's sessions
 *  (`prPictureSessionIds`), oldest first; sealed cards count through their
 *  clear part (`prPicturesOfCard`). `pullRequestId` = the PR rendered
 *  (every OTHER PR of the chat claims its opener's lineage); `head` = its
 *  branch: a PR with no opener resolves through it (`headSession`: the
 *  session credited with its latest push in `repositoryId`, else the one
 *  lineage on it, #EXP-1540). */
export async function loadPrPictures(
  rootId: string,
  recordedOpenerId: string | null,
  opts: { pullRequestId?: string | null; head?: string | null; repositoryId?: string | null } = {}
): Promise<PrPicture[]> {
  const tree = await db
    .select({
      id: sessions.id,
      parentId: sessions.parentId,
      continuedFromId: sessions.continuedFromId,
      status: sessions.status,
      branch: sessions.branch,
    })
    .from(sessions)
    .where(eq(sessions.rootId, rootId))
    .orderBy(asc(sessions.createdAt))
  const openerSessionId =
    recordedOpenerId ??
    (opts.head
      ? headSession(tree, opts.head, await headPusher({ repositoryId: opts.repositoryId, branch: opts.head }))
      : null)
  const pullRequestId = opts.pullRequestId ?? null
  const others = await db
    .select({ sessionId: pullRequests.sessionId })
    .from(pullRequests)
    .where(
      and(
        eq(pullRequests.rootSessionId, rootId),
        isNotNull(pullRequests.sessionId),
        ...(pullRequestId ? [ne(pullRequests.id, pullRequestId)] : [])
      )
    )
  const claimed = new Set<string>()
  for (const { sessionId } of others) {
    if (sessionId && sessionId !== openerSessionId) {
      for (const id of logicalSessionIds(sessionId, tree)) claimed.add(id)
    }
  }
  const ids = prPictureSessionIds(tree, openerSessionId, claimed)
  if (ids.size === 0) return []
  const rows = await db
    .select({ card: messages.card })
    .from(messages)
    .where(
      and(
        eq(messages.rootId, rootId),
        inSessions([...ids]),
        sql`${messages.authorSessionId} = ${messageSessionId()}`,
        sql`${messages.card}->>'kind' IN ('surface', 'question', 'plan')`
      )
    )
    .orderBy(asc(messages.createdAt))
  const base = appBaseUrl()
  const out: PrPicture[] = []
  for (const { card } of rows) {
    for (const picture of prPicturesOfCard(card)) {
      out.push({ ...picture, url: `${base}${buildAttachmentUrl(picture.attachmentId)}` })
    }
  }
  return out
}

/** The full commit messages of base...head, oldest first (GitHub caps a
 *  compare at 250 commits). Best effort: a failure = none. */
export async function compareCommitMessages(opts: {
  repo: string
  base: string
  head: string
  token: string
}): Promise<string[]> {
  try {
    const res = await githubFetch(`https://api.github.com/repos/${opts.repo}/compare/${encodeURIComponent(opts.base)}...${encodeURIComponent(opts.head)}`,
      { headers: githubApiHeaders(opts.token) }
    )
    if (!res.ok) return []
    const data = (await res.json()) as { commits?: { commit?: { message?: string } }[] }
    return (data.commits ?? []).map((entry) => entry.commit?.message ?? ``).filter(Boolean)
  } catch {
    return []
  }
}

/** The issues a PR links (identifier + title), in link order. */
export async function linkedIssuesOfPullRequest(
  pullRequestId: string
): Promise<{ identifier: string; title: string }[]> {
  return db
    .select({ identifier: issues.identifier, title: issues.title })
    .from(pullRequestIssues)
    .innerJoin(issues, eq(issues.id, pullRequestIssues.issueId))
    .where(and(eq(pullRequestIssues.pullRequestId, pullRequestId), eq(pullRequestIssues.closes, true), isNull(issues.deletedAt)))
    .orderBy(asc(pullRequestIssues.createdAt))
}

/** The footer link of every PR body: the chat (root session). */
export function chatUrlFor(teamSlug: string, rootId: string): string {
  return `${appBaseUrl()}${buildSessionDeepLinkPath(teamSlug, rootId)}`
}

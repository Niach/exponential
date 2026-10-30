import { and, asc, eq, sql } from "drizzle-orm"
import { isStoryCard } from "@exp/domain-contract/stories"
import type { MessageCard } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { issues, messages, pullRequestIssues, sessions } from "@/db/schema"
import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"
import { githubApiHeaders } from "@/lib/integrations/github-app"
import { appBaseUrl, buildSessionDeepLinkPath } from "@/lib/notification-email-policy"
import { logicalSessionIds } from "@/lib/sessions/guide-entries"
import { inSessions, messageSessionId } from "@/lib/sessions/messages"
import { isSealedCard } from "@/lib/sessions/sealed"

// S-3 (owner 2026-10-28): stories replace the Guide. The I/O half of a PR
// body (`prBodyFromStories`, lib/pr-body.ts): the opener's story pictures,
// the PR's commits and the thread link.

export interface StoryPicture {
  label: string
  attachmentId: string
  url: string
}

/** One story card's PR pictures (label + attachment). A plaintext card is
 *  read as is (`isStoryCard`). EXP-1468 (sealed-content addendum 5): a
 *  SEALED card is read through its CLEAR part only: the `pictures`
 *  projection's entries that carry an attachmentId, none when the clear
 *  `story` is false. Captions stay sealed: a PR body is a public export and
 *  never carries sealed text (the label stands in for the caption). */
export function storyPicturesOfCard(card: unknown): { label: string; attachmentId: string }[] {
  if (isSealedCard(card)) {
    const clear = card as Record<string, unknown>
    if (clear.kind !== `surface` && clear.kind !== `question` && clear.kind !== `plan`) return []
    if (clear.story === false || !Array.isArray(clear.pictures)) return []
    return clear.pictures.flatMap((entry: unknown) => {
      const { label, attachmentId } = (entry ?? {}) as Record<string, unknown>
      return typeof label === `string` && typeof attachmentId === `string` ? [{ label, attachmentId }] : []
    })
  }
  if (!isStoryCard(card)) return []
  return ((card as Extract<MessageCard, { kind: `surface` }>).pictures ?? []).flatMap((picture) =>
    picture.attachmentId ? [{ label: picture.label, attachmentId: picture.attachmentId }] : []
  )
}

/** The resolved pictures of the story cards the opener's LOGICAL session
 *  and its subtree posted (null opener = the whole tree), oldest first;
 *  sealed cards count through their clear part (`storyPicturesOfCard`). */
export async function loadStoryPictures(rootId: string, openerSessionId: string | null): Promise<StoryPicture[]> {
  const tree = await db
    .select({ id: sessions.id, parentId: sessions.parentId, branch: sessions.branch, status: sessions.status })
    .from(sessions)
    .where(eq(sessions.rootId, rootId))
  const ids = new Set(openerSessionId ? logicalSessionIds(openerSessionId, tree) : tree.map((row) => row.id))
  for (let grew = true; grew; ) {
    grew = false
    for (const row of tree) {
      if (row.parentId && ids.has(row.parentId) && !ids.has(row.id)) {
        ids.add(row.id)
        grew = true
      }
    }
  }
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
  const out: StoryPicture[] = []
  for (const { card } of rows) {
    for (const picture of storyPicturesOfCard(card)) {
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
    const res = await fetch(
      `https://api.github.com/repos/${opts.repo}/compare/${encodeURIComponent(opts.base)}...${encodeURIComponent(opts.head)}`,
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
    .where(eq(pullRequestIssues.pullRequestId, pullRequestId))
    .orderBy(asc(pullRequestIssues.createdAt))
}

/** The footer link of every PR body: the thread (root session). */
export function storiesUrlFor(teamSlug: string, rootId: string): string {
  return `${appBaseUrl()}${buildSessionDeepLinkPath(teamSlug, rootId)}`
}

import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import type { db } from "@/db/connection"
import { issues, messages, pullRequestIssues, pullRequests, sessionIssues, sessions } from "@/db/schema"
import { extractIssueRefs } from "@/lib/issue-refs"
import { inSession } from "@/lib/sessions/messages"

// EXP-1334/NEW-1: the issues a PR closes when `pr_open` names none. A
// session's issues are the rows it HOLDS; the PR belongs to ONE session of
// the tree, so it closes its SUBTREE's share: the rows its subtree holds
// plus the tree's issues its start message names as `#IDENT`. The root's
// own PR takes the whole thread; a child finding no share takes the rows
// held outside every OTHER live subtree (the root's own rows), so the one
// session doing the work keeps them when a review or fix-up child sits
// beside it. Done work (an issue a merged PR closed) is never re-linked.

type Reader = Pick<typeof db, `select`>

/** Pure: `sessionId` and its descendants in a tree's parent links. */
export function subtreeOf(
  tree: ReadonlyArray<{ id: string; parentId: string | null }>,
  sessionId: string
): Set<string> {
  const subtree = new Set([sessionId])
  let grew = true
  while (grew) {
    grew = false
    for (const node of tree) {
      if (node.parentId && subtree.has(node.parentId) && !subtree.has(node.id)) {
        subtree.add(node.id)
        grew = true
      }
    }
  }
  return subtree
}

/** Pure: the PR's share of the thread's rows (link order, each issue once). */
export function pickPrIssues(opts: {
  rootId: string
  sessionId: string | null
  links: ReadonlyArray<{ issueId: string; identifier: string; sessionId: string }>
  tree: ReadonlyArray<{ id: string; parentId: string | null; status?: string }>
  startText: string | null
}): string[] {
  const unique = (ids: string[]) => [...new Set(ids)]
  if (!opts.sessionId || opts.sessionId === opts.rootId) {
    return unique(opts.links.map((link) => link.issueId))
  }
  const subtree = subtreeOf(opts.tree, opts.sessionId)
  const named = new Set(extractIssueRefs(opts.startText ?? ``))
  const share = opts.links
    .filter((link) => subtree.has(link.sessionId) || named.has(link.identifier.toUpperCase()))
    .map((link) => link.issueId)
  if (share.length > 0) return unique(share)
  // Nothing of its own: the rows no OTHER live subtree holds (the root's).
  const others = new Set<string>()
  for (const node of opts.tree) {
    if (node.id === opts.rootId || subtree.has(node.id) || node.status === `ended`) continue
    if (node.parentId === opts.rootId) for (const id of subtreeOf(opts.tree, node.id)) others.add(id)
  }
  return unique(
    opts.links.filter((link) => !others.has(link.sessionId)).map((link) => link.issueId)
  )
}

/** The default issue set of a PR opened by `sessionId` in thread `rootId`. */
export async function prDefaultIssueIds(
  database: Reader,
  opts: { rootId: string; sessionId: string | null }
): Promise<string[]> {
  const tree = await database
    .select({ id: sessions.id, parentId: sessions.parentId, status: sessions.status })
    .from(sessions)
    .where(eq(sessions.rootId, opts.rootId))
  const links = await database
    .select({
      issueId: sessionIssues.issueId,
      identifier: issues.identifier,
      sessionId: sessionIssues.sessionId,
    })
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .where(inArray(sessionIssues.sessionId, tree.map((node) => node.id)))
    .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt))
  if (links.length === 0) return []

  let startText: string | null = null
  if (opts.sessionId && opts.sessionId !== opts.rootId) {
    // The session's start message: the first plain (non-card) row on it.
    const [start] = await database
      .select({ body: messages.body })
      .from(messages)
      .where(and(inSession(opts.sessionId), isNull(messages.card)))
      .orderBy(asc(messages.createdAt))
      .limit(1)
    startText = start?.body ?? null
  }
  const picked = pickPrIssues({
    rootId: opts.rootId,
    sessionId: opts.sessionId,
    links,
    tree,
    startText,
  })
  if (picked.length === 0) return []

  const done = await database
    .select({ issueId: pullRequestIssues.issueId })
    .from(pullRequestIssues)
    .innerJoin(pullRequests, eq(pullRequests.id, pullRequestIssues.pullRequestId))
    .where(and(inArray(pullRequestIssues.issueId, picked), eq(pullRequests.state, `merged`)))
  const doneIds = new Set(done.map((row) => row.issueId))
  return picked.filter((id) => !doneIds.has(id))
}

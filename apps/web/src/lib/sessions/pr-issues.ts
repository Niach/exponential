import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import type { db } from "@/db/connection"
import { issues, messages, pullRequestIssues, pullRequests, sessionIssues, sessions } from "@/db/schema"
import { extractIssueRefs } from "@/lib/issue-refs"

// EXP-1334: the issues a PR closes when `pr_open` names none. A thread's
// issues sit on its ROOT, but the PR belongs to ONE session of the tree, so
// it closes only that session's share: the links its subtree added
// (`added_by_session_id`) plus the root issues its start message names as
// `#IDENT`. The root's own PR, and the tree's ONLY work session finding no
// share, take the whole thread. Done work (an issue a merged PR closed) is
// never re-linked.

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

/** Pure: the PR's share of the thread's links (link order). */
export function pickPrIssues(opts: {
  rootId: string
  sessionId: string | null
  links: ReadonlyArray<{ issueId: string; identifier: string; addedBySessionId: string | null }>
  tree: ReadonlyArray<{ id: string; parentId: string | null; dispatch: boolean; aside: boolean }>
  startText: string | null
}): string[] {
  const all = opts.links.map((link) => link.issueId)
  if (!opts.sessionId || opts.sessionId === opts.rootId) return all
  const subtree = subtreeOf(opts.tree, opts.sessionId)
  const named = new Set(extractIssueRefs(opts.startText ?? ``))
  const share = opts.links
    .filter(
      (link) =>
        (link.addedBySessionId && subtree.has(link.addedBySessionId)) ||
        named.has(link.identifier.toUpperCase())
    )
    .map((link) => link.issueId)
  if (share.length > 0) return share
  // A dispatcher (and its continuations) only routes; the one session doing
  // the work owns the whole thread.
  const workers = opts.tree.filter(
    (node) => node.id !== opts.rootId && !node.dispatch && !node.aside
  )
  return workers.length === 1 && workers[0]!.id === opts.sessionId ? all : []
}

/** The default issue set of a PR opened by `sessionId` in thread `rootId`. */
export async function prDefaultIssueIds(
  database: Reader,
  opts: { rootId: string; sessionId: string | null }
): Promise<string[]> {
  const links = await database
    .select({
      issueId: sessionIssues.issueId,
      identifier: issues.identifier,
      addedBySessionId: sessionIssues.addedBySessionId,
    })
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .where(eq(sessionIssues.sessionId, opts.rootId))
    .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt))
  if (links.length === 0) return []

  let picked = links.map((link) => link.issueId)
  if (opts.sessionId && opts.sessionId !== opts.rootId) {
    const tree = await database
      .select({
        id: sessions.id,
        parentId: sessions.parentId,
        dispatch: sessions.dispatch,
        aside: sessions.aside,
      })
      .from(sessions)
      .where(eq(sessions.rootId, opts.rootId))
    // The session's start message: the first plain (non-card) row on it.
    const [start] = await database
      .select({ body: messages.body })
      .from(messages)
      .where(and(eq(messages.sessionId, opts.sessionId), isNull(messages.card)))
      .orderBy(asc(messages.createdAt))
      .limit(1)
    picked = pickPrIssues({
      rootId: opts.rootId,
      sessionId: opts.sessionId,
      links,
      tree,
      startText: start?.body ?? null,
    })
  }
  if (picked.length === 0) return []

  const done = await database
    .select({ issueId: pullRequestIssues.issueId })
    .from(pullRequestIssues)
    .innerJoin(pullRequests, eq(pullRequests.id, pullRequestIssues.pullRequestId))
    .where(and(inArray(pullRequestIssues.issueId, picked), eq(pullRequests.state, `merged`)))
  const doneIds = new Set(done.map((row) => row.issueId))
  return picked.filter((id) => !doneIds.has(id))
}

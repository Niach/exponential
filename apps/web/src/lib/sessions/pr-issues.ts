import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import type { db } from "@/db/connection"
import { issues, messages, pullRequestIssues, pullRequests, sessionIssues, sessions } from "@/db/schema"
import { extractIssueRefs } from "@/lib/issue-refs"
import { type GuideTreeRow, logicalSessionIds } from "@/lib/sessions/guide-entries"
import { inSession } from "@/lib/sessions/messages"
import { isSealedBody } from "@/lib/sessions/sealed"

// EXP-1334/NEW-1: the issues a PR closes when `pr_open` names none. A
// session's issues are the rows it HOLDS; the PR belongs to ONE session of
// the tree, so it closes its SUBTREE's share: the rows its subtree holds
// plus the tree's issues its start message names as `#IDENT` (the root's
// subtree = the whole chat; EXP-1706: the root's share is only the rows
// it holds that no other session holds). #EXP-1402 (FEED-166): a child with no share
// links NOTHING, never the chat's other issues (merging would close
// them). Done work (an issue a merged PR closed) is never re-linked.
// EXP-1458: a SEALED start message is never parsed; its clear
// `ref_issue_ids` (the writer's `refIssueIds`) names the issues instead, and
// a plaintext one honours them beside its `#IDENT`s.
// #EXP-1402: the share is the opener's LOGICAL session's (`logicalSessionIds`):
// a continuation ("fix the lint") inherits what its ended predecessors held
// and named in their briefs; a root in that lineage adds only its own rows,
// never its subtree (the whole chat).

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

/** A start message as the share reads it. */
export interface PrStart {
  text: string | null
  /** EXP-1458: the start message's `ref_issue_ids`. */
  refIssueIds?: readonly string[] | null
}

/** Pure: the PR's share of the chat's rows (link order, each issue once). */
export function pickPrIssues(opts: {
  rootId: string
  sessionId: string | null
  links: ReadonlyArray<{ issueId: string; identifier: string; sessionId: string }>
  tree: ReadonlyArray<GuideTreeRow>
  /** The opener's start message. */
  startText: string | null
  /** EXP-1458: the start message's `ref_issue_ids`. */
  startRefIssueIds?: readonly string[] | null
  /** #EXP-1402: the start messages of the opener's lineage predecessors. */
  lineageStarts?: readonly PrStart[]
}): string[] {
  const unique = (ids: string[]) => [...new Set(ids)]
  if (!opts.sessionId || opts.sessionId === opts.rootId) {
    // EXP-1706: never the whole chat: the root's own rows that no other
    // session of the chat holds (a child's work goes with its PR).
    const elsewhere = new Set(
      opts.links.filter((link) => link.sessionId !== opts.rootId).map((link) => link.issueId)
    )
    return unique(
      opts.links
        .filter((link) => link.sessionId === opts.rootId && !elsewhere.has(link.issueId))
        .map((link) => link.issueId)
    )
  }
  const held = new Set<string>()
  for (const id of logicalSessionIds(opts.sessionId, opts.tree)) {
    if (id === opts.rootId) held.add(id)
    else for (const member of subtreeOf(opts.tree, id)) held.add(member)
  }
  const starts: PrStart[] = [
    { text: opts.startText, refIssueIds: opts.startRefIssueIds },
    ...(opts.lineageStarts ?? []),
  ]
  const named = new Set(starts.flatMap((start) => extractIssueRefs(start.text ?? ``)))
  const refs = new Set(starts.flatMap((start) => [...(start.refIssueIds ?? [])]))
  return unique(
    opts.links
      .filter(
        (link) => held.has(link.sessionId) || named.has(link.identifier.toUpperCase()) || refs.has(link.issueId)
      )
      .map((link) => link.issueId)
  )
}

/** The default issue set of a PR opened by `sessionId` in chat `rootId`. */
export async function prDefaultIssueIds(
  database: Reader,
  opts: { rootId: string; sessionId: string | null }
): Promise<string[]> {
  const tree = await database
    .select({
      id: sessions.id,
      parentId: sessions.parentId,
      continuedFromId: sessions.continuedFromId,
      status: sessions.status,
    })
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
    .where(and(inArray(sessionIssues.sessionId, tree.map((node) => node.id)), isNull(issues.deletedAt)))
    .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt))
  if (links.length === 0) return []

  // The start message of each lineage member: its first plain (non-card)
  // row. EXP-1458: a sealed body is never parsed (its refs are the column).
  const starts: PrStart[] = []
  if (opts.sessionId && opts.sessionId !== opts.rootId) {
    for (const id of logicalSessionIds(opts.sessionId, tree)) {
      const [start] = await database
        .select({ body: messages.body, refIssueIds: messages.refIssueIds })
        .from(messages)
        .where(and(inSession(id), isNull(messages.card)))
        .orderBy(asc(messages.createdAt))
        .limit(1)
      starts.push({
        text: start && !isSealedBody(start.body) ? start.body : null,
        refIssueIds: start?.refIssueIds ?? null,
      })
    }
  }
  const picked = pickPrIssues({
    rootId: opts.rootId,
    sessionId: opts.sessionId,
    links,
    tree,
    startText: starts[0]?.text ?? null,
    startRefIssueIds: starts[0]?.refIssueIds ?? null,
    lineageStarts: starts.slice(1),
  })
  if (picked.length === 0) return []

  const done = await database
    .select({ issueId: pullRequestIssues.issueId })
    .from(pullRequestIssues)
    .innerJoin(pullRequests, eq(pullRequests.id, pullRequestIssues.pullRequestId))
    .where(
      and(inArray(pullRequestIssues.issueId, picked), eq(pullRequests.state, `merged`), eq(pullRequestIssues.closes, true))
    )
  const doneIds = new Set(done.map((row) => row.issueId))
  return picked.filter((id) => !doneIds.has(id))
}

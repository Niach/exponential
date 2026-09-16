// EXP-897: the SERVER's model of a PR stack.
//
// A stack is a chain of pull requests where each one's base branch is the head
// branch of the one below it. Exponential records that edge on the issue rows
// themselves (`issues.pr_base_branch`, synced; `issues.pr_stack_number`,
// server-only), so the chain is derivable WITHOUT GitHub — which is what makes
// nesting, "Merge stack" and the retarget-on-merge heal keep working on a repo
// where GitHub's stack preview is unavailable (a "candidate stack").
//
// The unit of a stack is a PULL REQUEST, not an issue: a batch PR links
// several issues to ONE prUrl and may itself be a stack member. Everything
// here therefore groups by `pr_url` first.
//
// The CLIENT-side mirror of the pure half lives in `lib/pr-stack.ts` (it works
// off the synced collections); this module additionally owns the DB loaders
// and the GitHub attach call, so it stays server-only.
import { and, eq, like } from "drizzle-orm"
import { issues } from "@/db/schema"
import { escapeLikePattern } from "@/lib/like-pattern"
import {
  addToStack,
  createStack,
  findStackForPull,
  type GitHubFetch,
} from "@/lib/integrations/github-pr"
import type { Context } from "@/lib/trpc"

/** The issue columns the stack model reads. */
export interface StackRow {
  id: string
  identifier: string
  title?: string | null
  status?: string | null
  branch: string | null
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  prBaseBranch: string | null
  prStackNumber: number | null
  /** EXP-897: the member's board — MCP grant confinement over a whole chain. */
  boardId?: string | null
}

/** One PR in a chain — the issues sharing it ride along (batch PRs). */
export interface StackEntry {
  prUrl: string
  prNumber: number | null
  prState: string | null
  branch: string | null
  baseBranch: string | null
  stackNumber: number | null
  issues: StackRow[]
}

function entryOf(rows: StackRow[]): StackEntry {
  const first = rows[0]!
  return {
    prUrl: first.prUrl!,
    prNumber: first.prNumber,
    prState: first.prState,
    branch: first.branch,
    baseBranch: first.prBaseBranch,
    // Any member of the batch carrying the number is the batch's number.
    stackNumber:
      rows.find((row) => row.prStackNumber != null)?.prStackNumber ?? null,
    issues: rows,
  }
}

/** Group rows into PR entries, preserving first-seen order. */
export function toStackEntries(rows: StackRow[]): StackEntry[] {
  const byUrl = new Map<string, StackRow[]>()
  for (const row of rows) {
    if (!row.prUrl) continue
    const bucket = byUrl.get(row.prUrl)
    if (bucket) bucket.push(row)
    else byUrl.set(row.prUrl, [row])
  }
  return [...byUrl.values()].map(entryOf)
}

/**
 * The full chain containing `fromPrUrl`, BOTTOM first. Walks down through
 * `baseBranch → branch` edges and up through the inverse; a base nobody in
 * `rows` owns ends the walk (that is the stack's landing branch), and a cycle
 * breaks where it first repeats. A PR that is in no chain comes back as a
 * one-element chain — every caller then reads "not stacked" off `size === 1`.
 */
export function orderStack(
  rows: StackRow[],
  fromPrUrl: string
): StackEntry[] {
  const entries = toStackEntries(rows)
  const byUrl = new Map(entries.map((entry) => [entry.prUrl, entry]))
  const start = byUrl.get(fromPrUrl)
  if (!start) return []

  // Both walks PREFER an open PR (FEED-43 R1): a closed-without-merge PR keeps
  // its rows until its edge is cleared, and an old PR on a re-cut branch keeps
  // the branch name, so "first match" could hang the chain on a dead member
  // while the live one on the same edge stayed out of "Merge stack". A closed
  // entry still stands in when nothing open does: the merged foundation of an
  // open member is part of its chain.
  const preferOpen = (candidates: StackEntry[]): StackEntry | undefined =>
    candidates.find((entry) => entry.prState === `open`) ?? candidates[0]

  const seen = new Set<string>([start.prUrl])
  const below: StackEntry[] = []
  let cursor = start
  for (;;) {
    // Only a non-empty branch identifies an entry: an empty/missing one
    // would otherwise make every base-less PR look like everyone's foundation.
    const base = cursor.baseBranch
    const lower = base
      ? preferOpen(
          entries.filter(
            (entry) => entry.branch === base && !seen.has(entry.prUrl)
          )
        )
      : undefined
    if (!lower) break
    seen.add(lower.prUrl)
    below.unshift(lower)
    cursor = lower
  }

  const above: StackEntry[] = []
  cursor = start
  for (;;) {
    const branch = cursor.branch
    const upper = branch
      ? preferOpen(
          entries.filter(
            (entry) => entry.baseBranch === branch && !seen.has(entry.prUrl)
          )
        )
      : undefined
    if (!upper) break
    seen.add(upper.prUrl)
    above.push(upper)
    cursor = upper
  }

  return [...below, start, ...above]
}

/** The chain's members at or below `prUrl` (bottom → top, inclusive). */
export function membersAtOrBelow(
  chain: StackEntry[],
  prUrl: string
): StackEntry[] {
  const index = chain.findIndex((entry) => entry.prUrl === prUrl)
  return index < 0 ? [] : chain.slice(0, index + 1)
}

/** The member directly ABOVE `prUrl`, or null at the top. */
export function memberAbove(
  chain: StackEntry[],
  prUrl: string
): StackEntry | null {
  const index = chain.findIndex((entry) => entry.prUrl === prUrl)
  if (index < 0) return null
  return chain[index + 1] ?? null
}

/** The topmost member of the chain (its last element). */
export function stackTop(chain: StackEntry[]): StackEntry | null {
  return chain.length > 0 ? chain[chain.length - 1]! : null
}

/**
 * The topmost member whose PR is still OPEN — the one a "merge the stack"
 * targets (merging it merges every unmerged member below it). Null when the
 * chain has no open member left.
 */
export function stackTopOpen(chain: StackEntry[]): StackEntry | null {
  for (let i = chain.length - 1; i >= 0; i -= 1) {
    if (chain[i]!.prState === `open`) return chain[i]!
  }
  return null
}

/** 1-based position from the bottom, and the chain's size. */
export function stackPosition(
  chain: StackEntry[],
  prUrl: string
): { position: number; size: number } | null {
  const index = chain.findIndex((entry) => entry.prUrl === prUrl)
  if (index < 0) return null
  return { position: index + 1, size: chain.length }
}

const PR_URL_PREFIX = `https://github.com/`

/** The LIKE pattern matching every PR url of one repository. */
export function prUrlPattern(repoFullName: string): string {
  return `${PR_URL_PREFIX}${escapeLikePattern(repoFullName)}/pull/%`
}

/**
 * Every issue of one team whose PR lives in `repoFullName` — the candidate
 * set a chain is derived from. Scoped to ONE repo because a stack lives in one
 * repository, and to one team because that is the access boundary.
 */
export async function loadStackRows(
  db: Context[`db`],
  opts: { teamId: string; repoFullName: string }
): Promise<StackRow[]> {
  return db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      title: issues.title,
      status: issues.status,
      branch: issues.branch,
      prUrl: issues.prUrl,
      prNumber: issues.prNumber,
      prState: issues.prState,
      prBaseBranch: issues.prBaseBranch,
      prStackNumber: issues.prStackNumber,
      boardId: issues.boardId,
    })
    .from(issues)
    .where(
      and(
        eq(issues.teamId, opts.teamId),
        like(issues.prUrl, prUrlPattern(opts.repoFullName))
      )
    )
}

export interface StackLower {
  issueId: string
  identifier: string
  prUrl: string
  prNumber: number
  branch: string
  prStackNumber: number | null
}

/**
 * The foundation an upper PR stacks ON: the lower issue's OPEN pull request in
 * the SAME repository. Every refusal names the issue, because the caller is
 * usually an agent reading the message (`exponential_pr_open`).
 */
export async function resolveStackLower(
  db: Context[`db`],
  opts: { lowerIssueId: string; repoFullName: string }
): Promise<StackLower> {
  const [row] = await db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      branch: issues.branch,
      prUrl: issues.prUrl,
      prNumber: issues.prNumber,
      prState: issues.prState,
      prStackNumber: issues.prStackNumber,
    })
    .from(issues)
    .where(eq(issues.id, opts.lowerIssueId))
    .limit(1)
  if (!row) throw new Error(`Issue not found`)
  if (!row.prUrl || row.prNumber == null || row.prState !== `open`) {
    throw new Error(`${row.identifier} has no open pull request to stack on.`)
  }
  const lowerRepo = repoFromPrUrl(row.prUrl)
  if (lowerRepo !== opts.repoFullName) {
    throw new Error(
      `${row.identifier}'s PR is on ${lowerRepo ?? `another repository`}, not ${opts.repoFullName} — a stack lives in one repository.`
    )
  }
  if (!row.branch) {
    throw new Error(`${row.identifier}'s PR has no head branch recorded`)
  }
  return {
    issueId: row.id,
    identifier: row.identifier,
    prUrl: row.prUrl,
    prNumber: row.prNumber,
    branch: row.branch,
    prStackNumber: row.prStackNumber,
  }
}

// Local copy of pr-sync's helper: importing that module here would drag the db
// connection (and its notification/relay imports) into every stack read.
function repoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/)
  return match ? match[1] : null
}

export interface SessionStackContext {
  onTopOfIssueId: string
  onTopOfIdentifier: string
  /** 1-based, from the bottom of the chain. */
  position: number
  size: number
}

/**
 * EXP-897: where a RUN's issue sits in its PR stack — what
 * `exponential_sessions_get` and every client's run header answer "2 of 3 · on
 * top of #EXP-11" from. Null for a run with no issue, no PR yet, or a PR that
 * is nobody's neighbour.
 */
export async function loadSessionStackContext(
  db: Context[`db`],
  opts: { issueId: string | null; teamId: string | null }
): Promise<SessionStackContext | null> {
  if (!opts.issueId || !opts.teamId) return null
  const [issue] = await db
    .select({ prUrl: issues.prUrl })
    .from(issues)
    .where(eq(issues.id, opts.issueId))
    .limit(1)
  if (!issue?.prUrl) return null
  const repoFullName = repoFromPrUrl(issue.prUrl)
  if (!repoFullName) return null
  const rows = await loadStackRows(db, { teamId: opts.teamId, repoFullName })
  const chain = orderStack(rows, issue.prUrl)
  const position = stackPosition(chain, issue.prUrl)
  if (!position || position.size < 2 || position.position < 2) return null
  const below = chain[position.position - 2]!
  return {
    onTopOfIssueId: below.issues[0]!.id,
    onTopOfIdentifier: below.issues
      .map((each) => each.identifier)
      .join(`, `),
    position: position.position,
    size: position.size,
  }
}

export interface AttachToStackResult {
  /** GitHub's stack number, or null when the preview is unavailable. */
  stackNumber: number | null
  /** True when THIS call created the stack (the lower row learns the number). */
  created: boolean
}

/**
 * Put `upperPrNumber` on top of `lower` on GitHub: extend the lower's existing
 * stack, or create a two-PR stack. NEVER throws for a GitHub-side refusal —
 * 404 (preview off) and 422 (GitHub disagrees the chain is valid) both come
 * back as `{stackNumber: null}` so the caller keeps the plain base-branch PR
 * it already created and records the edge on our side anyway.
 */
export async function attachToStack(opts: {
  repo: string
  token: string
  lower: { prNumber: number; stackNumber: number | null }
  upperPrNumber: number
  fetchImpl?: GitHubFetch
}): Promise<AttachToStackResult> {
  const { repo, token, lower, upperPrNumber, fetchImpl } = opts
  try {
    const known =
      lower.stackNumber ??
      (await findStackForPull(repo, lower.prNumber, token, fetchImpl))?.number ??
      null
    if (known != null) {
      const extended = await addToStack(
        repo,
        known,
        [upperPrNumber],
        token,
        fetchImpl
      )
      return { stackNumber: extended?.number ?? null, created: false }
    }
    const created = await createStack(
      repo,
      [lower.prNumber, upperPrNumber],
      token,
      fetchImpl
    )
    return {
      stackNumber: created?.number ?? null,
      created: created != null,
    }
  } catch {
    // A stack is an optimisation, never the contract: our own edge is already
    // recorded and the PR exists either way.
    return { stackNumber: null, created: false }
  }
}

// EXP-1248: GitHub NATIVE stacks, server side. A PR opened on another open
// PR's head becomes a member of a GitHub stack (REST `/stacks`), so GitHub
// draws the stack map, merges a member atomically with everything beneath it
// (`merge-async`) and rebases + retargets the rest itself. No fallback: a
// failed stack call throws (`GitHubStackError`) and the caller surfaces it.
//
// `pr_open` wires `inferBaseBranch` (base omitted) and `ensureGithubStack`
// (base = another open PR's head); `pr_merge({mergeStack})` and
// `issues.mergePr({mergeStack})` land a member through `mergeThrough`.
import {
  addToStack,
  compareRefs,
  createStack,
  findOpenPullByHead,
  getPullRequest,
  GitHubStackError,
  listOpenPulls,
  listStacksForPull,
  mergePullRequestSmart,
  type GitHubFetch,
  type GithubStack,
  type SmartMergeResult,
} from "@/lib/integrations/github-pr"

/** Cycle-safe bound on the walk down an existing chain (= MAX_STACK_DEPTH). */
const MAX_CHAIN = 10

function openMembers(stack: GithubStack): number[] {
  return stack.pulls
    .filter((pull) => pull.state === `open` && !pull.merged)
    .map((pull) => pull.number)
}

async function openStackOf(opts: {
  repo: string
  token: string
  prNumber: number
  fetchImpl?: GitHubFetch
}): Promise<GithubStack | null> {
  const stacks = await listStacksForPull(opts)
  return (
    stacks.find(
      (stack) => stack.open && openMembers(stack).includes(opts.prNumber)
    ) ?? null
  )
}

/**
 * Make `newPrNumber` the top of a GitHub stack sitting on `lowerPrNumber`.
 *
 * Already there → no call. `lowerPrNumber` tops an open stack → `add`. A
 * stack that continues ABOVE it is a fork GitHub cannot hold → error. Lower
 * in no stack → walk down its base refs while each base is another open PR's
 * head (stopping at `stopBranches`, the branches the repo is developed on, or
 * at a PR that already tops a stack) and stack the whole line bottom → top in
 * ONE call (`create`, or `add` onto the stack the walk ended on).
 */
export async function ensureGithubStack(opts: {
  repo: string
  token: string
  lowerPrNumber: number
  newPrNumber: number
  stopBranches?: readonly string[]
  fetchImpl?: GitHubFetch
}): Promise<GithubStack> {
  const { repo, token, fetchImpl } = opts
  const io = { repo, token, ...(fetchImpl ? { fetchImpl } : {}) }
  const stop = new Set(opts.stopBranches ?? [])

  const existing = await openStackOf({ ...io, prNumber: opts.lowerPrNumber })
  if (existing) {
    const open = openMembers(existing)
    if (open.includes(opts.newPrNumber)) return existing
    const top = open[open.length - 1]
    if (top !== opts.lowerPrNumber) {
      throw new GitHubStackError(
        409,
        `PR #${opts.lowerPrNumber} already has PR #${top} stacked on it in stack ${existing.number}; a GitHub stack is linear`
      )
    }
    return addToStack({ ...io, stackNumber: existing.number, numbers: [opts.newPrNumber] })
  }

  // Bottom → top: the line below `lowerPrNumber` that is in no stack yet.
  const line = [opts.lowerPrNumber, opts.newPrNumber]
  let anchor: GithubStack | null = null
  let current = opts.lowerPrNumber
  while (line.length < MAX_CHAIN + 1) {
    const pull = await getPullRequest(repo, current, token, fetchImpl)
    if (!pull.baseRef || stop.has(pull.baseRef)) break
    const below = await findOpenPullByHead(repo, pull.baseRef, token, fetchImpl)
    if (!below || line.includes(below.number)) break
    const belowStack = await openStackOf({ ...io, prNumber: below.number })
    if (belowStack) {
      const open = openMembers(belowStack)
      if (open[open.length - 1] !== below.number) {
        throw new GitHubStackError(
          409,
          `PR #${below.number} is mid-stack in stack ${belowStack.number}; PR #${current} cannot stack on it`
        )
      }
      anchor = belowStack
      break
    }
    line.unshift(below.number)
    current = below.number
  }
  return anchor
    ? addToStack({ ...io, stackNumber: anchor.number, numbers: line })
    : createStack({ ...io, numbers: line })
}

export interface InferredBase {
  /** The branch to open the PR on. */
  base: string
  /** The open PR whose head that is; null = the default branch. */
  prNumber: number | null
}

/**
 * `pr_open` without a `base`: the NEAREST open PR whose head is an ancestor
 * of `head` (compare API: `candidate...head` is `ahead`, fewest commits
 * ahead wins, the lower PR number on a tie), else `defaultBranch`.
 * `candidates` defaults to the repo's open PRs; a candidate whose branch is
 * gone (404) is skipped, any other GitHub error throws.
 */
export async function inferBaseBranch(opts: {
  repo: string
  token: string
  head: string
  defaultBranch: string
  candidates?: ReadonlyArray<{ number: number; branch: string }>
  fetchImpl?: GitHubFetch
}): Promise<InferredBase> {
  const candidates =
    opts.candidates ??
    (await listOpenPulls(opts.repo, opts.token, opts.fetchImpl)).map((pull) => ({
      number: pull.number,
      branch: pull.branch,
    }))
  let best: { number: number; branch: string; aheadBy: number } | null = null
  for (const candidate of candidates) {
    if (
      !candidate.branch ||
      candidate.branch === opts.head ||
      candidate.branch === opts.defaultBranch
    ) {
      continue
    }
    const comparison = await compareRefs({
      repo: opts.repo,
      base: candidate.branch,
      head: opts.head,
      token: opts.token,
      ...(opts.fetchImpl ? { fetchImpl: opts.fetchImpl } : {}),
    })
    if (comparison?.status !== `ahead` || comparison.aheadBy <= 0) continue
    if (
      !best ||
      comparison.aheadBy < best.aheadBy ||
      (comparison.aheadBy === best.aheadBy && candidate.number < best.number)
    ) {
      best = { ...candidate, aheadBy: comparison.aheadBy }
    }
  }
  return best
    ? { base: best.branch, prNumber: best.number }
    : { base: opts.defaultBranch, prNumber: null }
}

/**
 * Merge through a stack member: ONE merge-async on it, which lands it and
 * every open member beneath it (GitHub retargets the ones above). The poll
 * waits the stack deadline.
 */
export async function mergeThrough(opts: {
  repo: string
  token: string
  prNumber: number
  commitTitle?: string
  fetchImpl?: GitHubFetch
  sleepImpl?: (ms: number) => Promise<void>
  timeoutMs?: number
  stepMs?: number
}): Promise<SmartMergeResult> {
  return mergePullRequestSmart({ ...opts, stack: true })
}

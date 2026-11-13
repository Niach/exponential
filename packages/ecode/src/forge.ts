// eCode's forge (fixtures/forge.json is the spec): the PR-tree primitives
// every provider offers, as ONE pure function over a small repository model.
// Commits carry per-path changes (content or null = removed); a branch names
// a commit; a PR is a review unit whose base is ANY branch. eCode's own forge
// (the server's `ecodeForge`, the crate's `local` provider) behaves exactly
// like this; GitHub's is checked against the same fixture (#EXP-1614).
// ecode-core (Rust) is a port of this file and replays the same fixture.

import { isLive, reasonCode, supports, type ForgeReason, type Mergeability, type PrState, type Provider } from "./contract.generated"

export type Changes = Record<string, string | null>
export type Tree = Map<string, string>

export interface Commit {
  parent: string | null
  changes: Changes
}

export interface PullRequest {
  number: number
  head: string
  base: string
  title: string
  state: PrState
  /** The head's commit when it merged or closed. */
  headSha: string | null
  /** The squash commit on the base. */
  mergedSha: string | null
}

export interface ForgeState {
  provider: Provider
  defaultBranch: string
  commits: Record<string, Commit>
  branches: Record<string, string>
  prs: PullRequest[]
  nextPr: number
  nextCommit: number
}

export interface ForgeEventRow {
  kind: string
  [key: string]: unknown
}

export interface ForgeFailure {
  code: string
  reason: ForgeReason
}

export interface ForgeOutcome {
  state: ForgeState
  events: ForgeEventRow[]
  result?: Record<string, unknown>
  failure?: ForgeFailure
}

export interface ForgeGiven {
  provider?: Provider
  defaultBranch?: string
  commits?: Record<string, { parent?: string | null; changes?: Changes }>
  branches?: Record<string, string>
  prs?: Array<Partial<PullRequest> & { number: number; head: string; base: string }>
}

export function hydrate(given: ForgeGiven): ForgeState {
  const prs = (given.prs ?? []).map((pr) => ({
    number: pr.number,
    head: pr.head,
    base: pr.base,
    title: pr.title ?? ``,
    state: pr.state ?? `open`,
    headSha: pr.headSha ?? null,
    mergedSha: pr.mergedSha ?? null,
  }))
  const commits: Record<string, Commit> = {}
  for (const [id, commit] of Object.entries(given.commits ?? {})) {
    commits[id] = { parent: commit.parent ?? null, changes: { ...(commit.changes ?? {}) } }
  }
  return {
    provider: given.provider ?? `ecode`,
    defaultBranch: given.defaultBranch ?? `main`,
    commits,
    branches: { ...(given.branches ?? {}) },
    prs,
    nextPr: prs.reduce((max, pr) => Math.max(max, pr.number), 0) + 1,
    nextCommit: 1,
  }
}

function clone(state: ForgeState): ForgeState {
  return {
    ...state,
    commits: { ...state.commits },
    branches: { ...state.branches },
    prs: state.prs.map((pr) => ({ ...pr })),
  }
}

class Refused extends Error {
  constructor(readonly reason: ForgeReason) {
    super(reason)
  }
}

const refuse = (reason: ForgeReason): never => {
  throw new Refused(reason)
}

// ── the git model ───────────────────────────────────────────────────────

/** The commit and its ancestors, newest first. */
export function ancestry(state: ForgeState, id: string): string[] {
  const chain: string[] = []
  let cursor: string | null = id
  while (cursor !== null && !chain.includes(cursor)) {
    chain.push(cursor)
    cursor = state.commits[cursor]?.parent ?? null
  }
  return chain
}

/** The newest common ancestor of two commits, null = unrelated histories. */
export function mergeBase(state: ForgeState, a: string, b: string): string | null {
  const seen = new Set(ancestry(state, a))
  return ancestry(state, b).find((id) => seen.has(id)) ?? null
}

/** The whole tree at a commit (null = the empty tree). */
export function snapshot(state: ForgeState, id: string | null): Tree {
  const tree: Tree = new Map()
  if (id === null) return tree
  for (const commitId of ancestry(state, id).reverse()) {
    for (const [path, content] of Object.entries(state.commits[commitId]?.changes ?? {})) {
      if (content === null) tree.delete(path)
      else tree.set(path, content)
    }
  }
  return tree
}

const sortedPaths = (...trees: Tree[]): string[] => [...new Set(trees.flatMap((tree) => [...tree.keys()]))].sort(byCodePoint)

/** Code-point order (Rust's `str` order), never locale or UTF-16 order. */
export function byCodePoint(a: string, b: string): number {
  const x = Array.from(a)
  const y = Array.from(b)
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const d = x[i]!.codePointAt(0)! - y[i]!.codePointAt(0)!
    if (d !== 0) return d < 0 ? -1 : 1
  }
  return x.length - y.length < 0 ? -1 : x.length > y.length ? 1 : 0
}

/** A three-way merge of `ours` and `theirs` over `origin`, path by path. */
export function threeWay(origin: Tree, ours: Tree, theirs: Tree): { tree: Tree; conflicts: string[] } {
  const tree: Tree = new Map()
  const conflicts: string[] = []
  for (const path of sortedPaths(origin, ours, theirs)) {
    const o = origin.get(path) ?? null
    const a = ours.get(path) ?? null
    const b = theirs.get(path) ?? null
    let merged: string | null
    if (a === o) merged = b
    else if (b === o || a === b) merged = a
    else {
      conflicts.push(path)
      merged = a
    }
    if (merged !== null) tree.set(path, merged)
  }
  return { tree, conflicts }
}

/** The per-path changes that turn `from` into `to`, in path order. */
export function diffTrees(from: Tree, to: Tree): Changes {
  const changes: Changes = {}
  for (const path of sortedPaths(from, to)) {
    const before = from.get(path) ?? null
    const after = to.get(path) ?? null
    if (before !== after) changes[path] = after
  }
  return changes
}

function tip(state: ForgeState, branch: string, reason: ForgeReason): string {
  return state.branches[branch] ?? refuse(reason)
}

function livePr(state: ForgeState, number: unknown): PullRequest {
  const pr = state.prs.find((candidate) => candidate.number === number) ?? refuse(`pr`)
  if (!isLive(pr.state)) refuse(`not_open`)
  return pr
}

/** Would basing a PR with head `head` on `base` close a loop of live PRs? */
function closesLoop(state: ForgeState, head: string, base: string): boolean {
  let cursor = base
  for (let hops = 0; hops <= state.prs.length; hops++) {
    if (cursor === head) return true
    const parent = state.prs.find((pr) => isLive(pr.state) && pr.head === cursor)
    if (!parent) return false
    cursor = parent.base
  }
  return true
}

function mergeability(state: ForgeState, pr: PullRequest): { mergeability: Mergeability; conflicts: string[]; tree: Tree; base: string } {
  const head = tip(state, pr.head, `head`)
  const base = tip(state, pr.base, `base`)
  const origin = mergeBase(state, head, base)
  const { tree, conflicts } = threeWay(snapshot(state, origin), snapshot(state, base), snapshot(state, head))
  return { mergeability: conflicts.length === 0 ? `clean` : `dirty`, conflicts, tree, base }
}

function newCommitId(state: ForgeState, prefix: string): string {
  let id = `${prefix}${state.nextCommit}`
  while (state.commits[id]) {
    state.nextCommit += 1
    id = `${prefix}${state.nextCommit}`
  }
  state.nextCommit += 1
  return id
}

// ── the ops ─────────────────────────────────────────────────────────────

function squashMerge(state: ForgeState, events: ForgeEventRow[], number: unknown, expectedHead: unknown): string {
  const pr = livePr(state, number)
  if (pr.state === `draft`) refuse(`draft`)
  const head = tip(state, pr.head, `head`)
  if (typeof expectedHead === `string` && expectedHead !== head) refuse(`head_moved`)
  const merge = mergeability(state, pr)
  if (merge.mergeability === `dirty`) refuse(`dirty`)
  const id = newCommitId(state, `m`)
  state.commits[id] = { parent: merge.base, changes: diffTrees(snapshot(state, merge.base), merge.tree) }
  state.branches[pr.base] = id
  pr.state = `merged`
  pr.headSha = head
  pr.mergedSha = id
  events.push({ kind: `pr_merged`, number: pr.number, sha: id })
  // The PR tree: every live child moves onto the merged PR's base (EXP-324).
  for (const child of state.prs) {
    if (isLive(child.state) && child.base === pr.head) {
      child.base = pr.base
      events.push({ kind: `pr_retargeted`, number: child.number, from: pr.head, to: pr.base })
    }
  }
  if (pr.head !== state.defaultBranch && !state.prs.some((other) => isLive(other.state) && other.head === pr.head)) {
    delete state.branches[pr.head]
    events.push({ kind: `branch_deleted`, branch: pr.head })
  }
  return id
}

type Args = Record<string, unknown>

function apply(state: ForgeState, op: string, args: Args, events: ForgeEventRow[]): Record<string, unknown> {
  switch (op) {
    case `push`: {
      const branch = String(args.branch)
      let target: string
      const commit = args.commit as { id: string; parent?: string | null; changes?: Changes } | undefined
      if (commit) {
        if (state.commits[commit.id]) refuse(`commit_exists`)
        const parent = commit.parent ?? null
        if (parent !== null && !state.commits[parent]) refuse(`parent`)
        target = commit.id
        const current = state.branches[branch]
        const fastForward = current === undefined || (parent !== null && ancestry(state, parent).includes(current))
        if (!fastForward && args.force !== true) refuse(`non_fast_forward`)
        state.commits[commit.id] = { parent, changes: { ...(commit.changes ?? {}) } }
      } else {
        target = String(args.sha)
        if (!state.commits[target]) refuse(`commit`)
        const current = state.branches[branch]
        if (current !== undefined && args.force !== true && !ancestry(state, target).includes(current)) refuse(`non_fast_forward`)
      }
      state.branches[branch] = target
      events.push({ kind: `pushed`, branch, sha: target })
      return { sha: target }
    }
    case `pr.open`: {
      const head = String(args.head)
      const base = String(args.base)
      tip(state, head, `head`)
      tip(state, base, `base`)
      if (head === base) refuse(`same_branch`)
      if (state.prs.some((pr) => isLive(pr.state) && pr.head === head)) refuse(`exists`)
      if (closesLoop(state, head, base)) refuse(`cycle`)
      const number = state.nextPr
      state.nextPr += 1
      state.prs.push({
        number,
        head,
        base,
        title: String(args.title ?? ``),
        state: args.draft === true ? `draft` : `open`,
        headSha: null,
        mergedSha: null,
      })
      events.push({ kind: `pr_opened`, number })
      return { number }
    }
    case `pr.list_by_base`: {
      const base = String(args.base)
      const numbers = state.prs
        .filter((pr) => isLive(pr.state) && pr.base === base)
        .map((pr) => pr.number)
        .sort((a, b) => a - b)
      return { numbers }
    }
    case `pr.retarget`: {
      const pr = livePr(state, args.number)
      const base = String(args.base)
      tip(state, base, `base`)
      if (base === pr.head) refuse(`same_branch`)
      if (!supports(state.provider, `retarget`)) refuse(`unsupported`)
      if (base === pr.base) return {}
      if (closesLoop(state, pr.head, base)) refuse(`cycle`)
      events.push({ kind: `pr_retargeted`, number: pr.number, from: pr.base, to: base })
      pr.base = base
      return {}
    }
    case `pr.mergeability`: {
      const pr = livePr(state, args.number)
      const merge = mergeability(state, pr)
      return { mergeability: merge.mergeability, conflicts: merge.conflicts }
    }
    case `pr.squash_merge`:
      return { sha: squashMerge(state, events, args.number, args.expectedHead) }
    case `pr.merge_ordered`: {
      const numbers = (args.numbers as unknown[]) ?? []
      if (new Set(numbers).size !== numbers.length) refuse(`duplicate`)
      for (const number of numbers) livePr(state, number)
      const merged: Array<{ number: unknown; sha: string }> = []
      for (const number of numbers) {
        // Each merge is its own transaction: a refusal leaves the earlier ones landed.
        const before = clone(state)
        const eventCount = events.length
        try {
          merged.push({ number, sha: squashMerge(state, events, number, null) })
        } catch (error) {
          if (!(error instanceof Refused)) throw error
          Object.assign(state, before)
          events.length = eventCount
          return { merged, failed: { number, code: reasonCode(error.reason), reason: error.reason } }
        }
      }
      return { merged, failed: null }
    }
    case `pr.close`: {
      const pr = livePr(state, args.number)
      pr.state = `closed`
      pr.headSha = state.branches[pr.head] ?? null
      events.push({ kind: `pr_closed`, number: pr.number })
      return {}
    }
    case `pr.files`: {
      const pr = livePr(state, args.number)
      const head = tip(state, pr.head, `head`)
      const base = tip(state, pr.base, `base`)
      const before = snapshot(state, mergeBase(state, head, base))
      const after = snapshot(state, head)
      // Path order from the sorted union (an object would put number-like keys first).
      const files = sortedPaths(before, after)
        .filter((path) => (before.get(path) ?? null) !== (after.get(path) ?? null))
        .map((path) => ({ path, change: !after.has(path) ? `removed` : before.has(path) ? `modified` : `added` }))
      return { files }
    }
    case `branch.rebase_onto`: {
      const branch = String(args.branch)
      const from = tip(state, branch, `head`)
      const onto = tip(state, String(args.onto), `base`)
      const upstream = String(args.upstream)
      const chain = ancestry(state, from)
      const stop = chain.indexOf(upstream)
      if (stop < 0) refuse(`upstream`)
      let tree = snapshot(state, onto)
      let parent = onto
      for (const id of chain.slice(0, stop).reverse()) {
        const original = state.commits[id] ?? { parent: null, changes: {} }
        const before = snapshot(state, original.parent)
        const next = new Map(tree)
        for (const [path, content] of Object.entries(original.changes)) {
          const was = before.get(path) ?? null
          const now = tree.get(path) ?? null
          if (now === was || now === content) {
            if (content === null) next.delete(path)
            else next.set(path, content)
          } else refuse(`dirty`)
        }
        const rebased = newCommitId(state, `r`)
        state.commits[rebased] = { parent, changes: diffTrees(tree, next) }
        tree = next
        parent = rebased
      }
      state.branches[branch] = parent
      events.push({ kind: `pushed`, branch, sha: parent, forced: true })
      return { sha: parent }
    }
    default:
      throw new Error(`unknown forge op ${op}`)
  }
}

/** One op on the forge: the next state, the events it emits and its result,
 *  or a refusal (state unchanged, no events). */
export function forge(state: ForgeState, op: string, args: Args): ForgeOutcome {
  const next = clone(state)
  const events: ForgeEventRow[] = []
  try {
    const result = apply(next, op, args, events)
    return { state: next, events, result }
  } catch (error) {
    if (!(error instanceof Refused)) throw error
    return { state, events: [], failure: { code: reasonCode(error.reason), reason: error.reason } }
  }
}

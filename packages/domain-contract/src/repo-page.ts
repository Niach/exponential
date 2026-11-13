// #EXP-1616 THE REPO PAGE ×4 (fixtures/repo-page.json: its `$comment` is the
// spec). Pure folds every host replays: the PR tree (`repoPrTree`), the
// branch list (`repoBranches`) and a repo row's checks rollup on Settings ›
// Repositories (`repoChecksRollup`). Rust: domain::repo_page; Swift: ExpCore
// RepoPage; Kotlin: com.exponential.app.domain.RepoPage.
import fixture from "../fixtures/repo-page.json" with { type: "json" }

export const REPO_PAGE_COPY = fixture.copy
export const REPO_TABS = fixture.tabs as ReadonlyArray<{ id: RepoTab; label: string }>
export type RepoTab = `pulls` | `runs` | `branches` | `settings`

export interface RepoPr {
  id: string
  number: number
  title: string | null
  head: string
  base: string
  state: string
  checksState: string
}

export interface RepoPrRow {
  id: string
  number: number
  /** `#<number>`. */
  identifier: string
  title: string
  depth: number
  /** `<head> → <base>`. */
  subtitle: string
  draft: boolean
  /** The rollup chip; null = no checks (no chip). */
  checks: string | null
}

const LIVE = new Set([`open`, `draft`])

/** Code-point order (Rust's `str` order). */
function byCodePoint(a: string, b: string): number {
  const x = Array.from(a)
  const y = Array.from(b)
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const d = x[i]!.codePointAt(0)! - y[i]!.codePointAt(0)!
    if (d !== 0) return d < 0 ? -1 : 1
  }
  return x.length - y.length
}

/** The live PRs as a tree: a PR's parent is the live PR whose head is its
 *  base. Roots newest first; children oldest first, depth-first. */
export function repoPrTree(prs: readonly RepoPr[]): RepoPrRow[] {
  const live = prs.filter((pr) => LIVE.has(pr.state))
  const byHead = new Map<string, RepoPr>()
  for (const pr of [...live].sort((a, b) => a.number - b.number)) if (!byHead.has(pr.head)) byHead.set(pr.head, pr)
  const parentOf = (pr: RepoPr): RepoPr | null => {
    const parent = byHead.get(pr.base)
    return parent && parent.id !== pr.id ? parent : null
  }
  const children = new Map<string, RepoPr[]>()
  const roots: RepoPr[] = []
  for (const pr of live) {
    const parent = parentOf(pr)
    if (parent) children.set(parent.id, [...(children.get(parent.id) ?? []), pr])
    else roots.push(pr)
  }
  const rows: RepoPrRow[] = []
  const seen = new Set<string>()
  const visit = (pr: RepoPr, depth: number) => {
    if (seen.has(pr.id)) return
    seen.add(pr.id)
    rows.push({
      id: pr.id,
      number: pr.number,
      identifier: `#${pr.number}`,
      title: pr.title ?? ``,
      depth,
      subtitle: `${pr.head} → ${pr.base}`,
      draft: pr.state === `draft`,
      checks: pr.checksState === `none` ? null : pr.checksState,
    })
    for (const child of (children.get(pr.id) ?? []).sort((a, b) => a.number - b.number)) visit(child, depth + 1)
  }
  for (const root of roots.sort((a, b) => b.number - a.number)) visit(root, 0)
  // A loop of live PRs (no root): its newest member starts it at depth 0.
  for (const pr of [...live].sort((a, b) => b.number - a.number)) visit(pr, 0)
  return rows
}

export interface RepoBranchRow {
  name: string
  /** The first 7 characters of the sha. */
  sha: string
  isDefault: boolean
  /** The live PR whose head this branch is, null = none. */
  prNumber: number | null
}

/** The default branch first, then by name (code points). */
export function repoBranches(
  branches: ReadonlyArray<{ name: string; sha: string }>,
  defaultBranch: string,
  prs: readonly RepoPr[]
): RepoBranchRow[] {
  const prOf = (name: string) =>
    prs.filter((pr) => LIVE.has(pr.state) && pr.head === name).sort((a, b) => a.number - b.number)[0]?.number ?? null
  return [...branches]
    .sort((a, b) => (a.name === defaultBranch ? -1 : b.name === defaultBranch ? 1 : byCodePoint(a.name, b.name)))
    .map((b) => ({ name: b.name, sha: b.sha.slice(0, 7), isDefault: b.name === defaultBranch, prNumber: prOf(b.name) }))
}

/** A repo row's checks chip over its live PRs: failure (any) · pending (any)
 *  · success (all of the ones with checks) · null = none has checks. */
export function repoChecksRollup(prs: readonly RepoPr[]): string | null {
  const states = prs.filter((pr) => LIVE.has(pr.state) && pr.checksState !== `none`).map((pr) => pr.checksState)
  if (states.length === 0) return null
  if (states.includes(`failure`)) return `failure`
  if (states.includes(`pending`)) return `pending`
  return `success`
}

/** The tab a route's `tab` search param opens (unknown = pulls). */
export function repoTab(param: string | null | undefined): RepoTab {
  return REPO_TABS.some((tab) => tab.id === param) ? (param as RepoTab) : `pulls`
}

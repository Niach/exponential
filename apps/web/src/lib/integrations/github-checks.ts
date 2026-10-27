import { githubApiHeaders } from "@/lib/integrations/github-app"
import type { GitHubFetch } from "@/lib/integrations/github-pr"

// EXP-1378: THE check gate in front of every merge we make (the person's
// Merge PR / Merge stack, `exponential_pr merge`, the automerge). GitHub's
// own auto-merge waits for the REQUIRED checks; our merge call does not, so
// it reads them first. Required = the base branch's rulesets plus classic
// branch protection; a repo that requires nothing gates on every check that
// reported on the head commit. ANY read that fails (no `checks`/`statuses`
// permission on the App, a rate limit, GitHub down; a 404 on branch
// protection only means none) answers null and the merge path refuses
// (fails closed, `checksUnreadableRefusal`): a person may still Merge
// anyway. The verdict carries the head sha it judged; the merge pins it.

export type ChecksVerdict =
  | { state: `green` }
  | { state: `pending`; names: string[] }
  | { state: `red`; names: string[] }

export interface CheckRunRow {
  name: string
  /** queued | in_progress | completed (| waiting | requested | pending). */
  status: string
  /** success | failure | neutral | cancelled | skipped | timed_out | action_required | stale | null */
  conclusion: string | null
}

export interface CommitStatusRow {
  context: string
  /** success | pending | failure | error */
  state: string
}

const PASSING = new Set([`success`, `neutral`, `skipped`])

type One = `green` | `pending` | `red`

function runState(run: CheckRunRow): One {
  if (run.status !== `completed`) return `pending`
  return run.conclusion && PASSING.has(run.conclusion) ? `green` : `red`
}

function statusState(status: CommitStatusRow): One {
  if (status.state === `success`) return `green`
  if (status.state === `pending`) return `pending`
  return `red`
}

/** Pure: the gate's verdict. `required` empty = every reported check counts;
 *  a required check that has not reported yet is pending ("expected"). Red
 *  wins over pending: a failed check never turns green by waiting. */
export function evaluateChecks(
  required: readonly string[],
  runs: readonly CheckRunRow[],
  statuses: readonly CommitStatusRow[]
): ChecksVerdict {
  const byName = new Map<string, One>()
  // GitHub lists the newest run of a name first; a re-run supersedes.
  for (const run of runs) if (!byName.has(run.name)) byName.set(run.name, runState(run))
  for (const status of statuses) {
    if (!byName.has(status.context)) byName.set(status.context, statusState(status))
  }
  const names = required.length > 0 ? [...new Set(required)] : [...byName.keys()]
  const pending: string[] = []
  const red: string[] = []
  for (const name of names) {
    const state = byName.get(name) ?? `pending`
    if (state === `pending`) pending.push(name)
    else if (state === `red`) red.push(name)
  }
  if (red.length > 0) return { state: `red`, names: red }
  if (pending.length > 0) return { state: `pending`, names: pending }
  return { state: `green` }
}

type Read = { ok: true; data: unknown } | { ok: false; status: number }

async function read(doFetch: GitHubFetch, url: string, token: string | null | undefined): Promise<Read> {
  const res = await doFetch(url, { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) })
  if (!res.ok) return { ok: false, status: res.status }
  return { ok: true, data: await res.json() }
}

/** Thrown inside the gate: a read it cannot do without (a 403/429/5xx), so
 *  the verdict is null. */
class Unreadable extends Error {}

async function readOrThrow(doFetch: GitHubFetch, url: string, token: string | null | undefined): Promise<unknown> {
  const answer = await read(doFetch, url, token)
  if (!answer.ok) throw new Unreadable(`HTTP ${answer.status} on ${url}`)
  return answer.data
}

const PAGE = 100
/** A head with more than 1000 checks is not a head we judge. */
const MAX_PAGES = 10

/** Every page of a commit's check runs / statuses (`key` = the list field). */
async function readAll<T>(
  doFetch: GitHubFetch,
  url: string,
  key: `check_runs` | `statuses`,
  token: string | null | undefined
): Promise<T[]> {
  const rows: T[] = []
  for (let page = 1; ; page++) {
    if (page > MAX_PAGES) throw new Unreadable(`more than ${MAX_PAGES} pages on ${url}`)
    const data = (await readOrThrow(doFetch, `${url}?per_page=${PAGE}&page=${page}`, token)) as {
      total_count?: number
    } & Record<string, unknown>
    const list = Array.isArray(data?.[key]) ? (data[key] as T[]) : []
    rows.push(...list)
    const total = typeof data?.total_count === `number` ? data.total_count : null
    if (list.length < PAGE || (total != null && rows.length >= total)) return rows
  }
}

/** The base branch's required check names (rulesets + classic protection).
 *  A 404 = nothing of that kind configured; any other failure throws (an
 *  unreadable requirement never reads as "none required"). */
async function requiredChecks(
  doFetch: GitHubFetch,
  repo: string,
  base: string,
  token: string | null | undefined
): Promise<string[]> {
  const branch = encodeURIComponent(base)
  const optional = async (url: string): Promise<unknown> => {
    const answer = await read(doFetch, url, token)
    if (answer.ok) return answer.data
    if (answer.status === 404) return null
    throw new Unreadable(`HTTP ${answer.status} on ${url}`)
  }
  const [rules, protection] = await Promise.all([
    optional(`https://api.github.com/repos/${repo}/rules/branches/${branch}`),
    optional(`https://api.github.com/repos/${repo}/branches/${branch}/protection/required_status_checks`),
  ])
  const names: string[] = []
  if (Array.isArray(rules)) {
    for (const rule of rules as { type?: string; parameters?: { required_status_checks?: { context?: string }[] } }[]) {
      if (rule.type !== `required_status_checks`) continue
      for (const check of rule.parameters?.required_status_checks ?? []) {
        if (check.context) names.push(check.context)
      }
    }
  }
  if (protection && typeof protection === `object`) {
    const classic = protection as { contexts?: string[]; checks?: { context?: string }[] }
    for (const context of classic.contexts ?? []) names.push(context)
    for (const check of classic.checks ?? []) if (check.context) names.push(check.context)
  }
  return [...new Set(names)]
}

/** The gate's answer: the verdict over the head commit it read (`sha`). The
 *  merge pins that sha, so a push after the read is refused by GitHub. */
export type PullRequestChecks = ChecksVerdict & { sha: string }

/** The check gate for one open PR: its head commit against its base's
 *  required checks. Null = the gate could not read GitHub (the merge
 *  refuses): the PR, the required checks (a 404 = none), or EITHER list
 *  failed (403 permission, 429 rate limit, 5xx). Never a partial verdict. */
export async function pullRequestChecks(opts: {
  repo: string
  prNumber: number
  token: string | null | undefined
  fetchImpl?: GitHubFetch
}): Promise<PullRequestChecks | null> {
  const doFetch = opts.fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  try {
    const pull = (await readOrThrow(
      doFetch,
      `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
      opts.token
    )) as { head?: { sha?: string }; base?: { ref?: string } } | null
    const sha = pull?.head?.sha
    const base = pull?.base?.ref
    if (!sha || !base) return null
    const commit = `https://api.github.com/repos/${opts.repo}/commits/${sha}`
    const [required, runs, statuses] = await Promise.all([
      requiredChecks(doFetch, opts.repo, base, opts.token),
      readAll<CheckRunRow>(doFetch, `${commit}/check-runs`, `check_runs`, opts.token),
      readAll<CommitStatusRow>(doFetch, `${commit}/status`, `statuses`, opts.token),
    ])
    return { ...evaluateChecks(required, runs, statuses), sha }
  } catch (err) {
    console.warn(`[checks] ${opts.repo}#${opts.prNumber}:`, err instanceof Error ? err.message : err)
    return null
  }
}

/** The PR's current head commit (null = unreadable). */
export async function pullRequestHeadSha(opts: {
  repo: string
  prNumber: number
  token: string | null | undefined
  fetchImpl?: GitHubFetch
}): Promise<string | null> {
  const doFetch = opts.fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  try {
    const answer = await read(doFetch, `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`, opts.token)
    if (!answer.ok) return null
    return (answer.data as { head?: { sha?: string } } | null)?.head?.sha ?? null
  } catch {
    return null
  }
}

/** GitHub's own refusal names a required check it is still waiting for
 *  ("Required status check "gate" is expected.", a ruleset violation). */
export function isRequiredCheckRefusal(message: string): boolean {
  return /required status checks?\b/i.test(message)
}

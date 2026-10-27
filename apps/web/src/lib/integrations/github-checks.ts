import { githubApiHeaders } from "@/lib/integrations/github-app"
import type { GitHubFetch } from "@/lib/integrations/github-pr"

// EXP-1378: THE check gate in front of every merge we make (the person's
// Merge PR / Merge stack, `exponential_pr merge`, the automerge). GitHub's
// own auto-merge waits for the REQUIRED checks; our merge call does not, so
// it reads them first. Required = the base branch's rulesets plus classic
// branch protection; a repo that requires nothing gates on every check that
// reported on the head commit. A read that fails (no `checks`/`statuses`
// permission on the App, GitHub down) answers null and the merge path
// refuses (fails closed, `checksUnreadableRefusal`): a person may still
// Merge anyway.

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

async function readJson(
  doFetch: GitHubFetch,
  url: string,
  token: string | null | undefined
): Promise<unknown | null> {
  const res = await doFetch(url, { headers: githubApiHeaders(token || process.env.GITHUB_TOKEN) })
  if (!res.ok) return null
  return res.json()
}

/** The base branch's required check names (rulesets + classic protection). */
async function requiredChecks(
  doFetch: GitHubFetch,
  repo: string,
  base: string,
  token: string | null | undefined
): Promise<string[]> {
  const branch = encodeURIComponent(base)
  const [rules, protection] = await Promise.all([
    readJson(doFetch, `https://api.github.com/repos/${repo}/rules/branches/${branch}`, token),
    readJson(
      doFetch,
      `https://api.github.com/repos/${repo}/branches/${branch}/protection/required_status_checks`,
      token
    ),
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

/** The check gate for one open PR: its head commit against its base's
 *  required checks. Null = the gate could not read GitHub (the merge
 *  refuses). */
export async function pullRequestChecks(opts: {
  repo: string
  prNumber: number
  token: string | null | undefined
  fetchImpl?: GitHubFetch
}): Promise<ChecksVerdict | null> {
  const doFetch = opts.fetchImpl ?? (globalThis.fetch as unknown as GitHubFetch)
  try {
    const pull = (await readJson(
      doFetch,
      `https://api.github.com/repos/${opts.repo}/pulls/${opts.prNumber}`,
      opts.token
    )) as { head?: { sha?: string }; base?: { ref?: string } } | null
    const sha = pull?.head?.sha
    const base = pull?.base?.ref
    if (!sha || !base) return null
    const [required, runs, statuses] = await Promise.all([
      requiredChecks(doFetch, opts.repo, base, opts.token),
      readJson(
        doFetch,
        `https://api.github.com/repos/${opts.repo}/commits/${sha}/check-runs?per_page=100`,
        opts.token
      ) as Promise<{ check_runs?: CheckRunRow[] } | null>,
      readJson(
        doFetch,
        `https://api.github.com/repos/${opts.repo}/commits/${sha}/status?per_page=100`,
        opts.token
      ) as Promise<{ statuses?: CommitStatusRow[] } | null>,
    ])
    // Neither list readable (the App lacks the permission): no verdict.
    if (!runs && !statuses) return null
    return evaluateChecks(required, runs?.check_runs ?? [], statuses?.statuses ?? [])
  } catch (err) {
    console.warn(`[checks] ${opts.repo}#${opts.prNumber}:`, err)
    return null
  }
}

/** GitHub's own refusal names a required check it is still waiting for
 *  ("Required status check "gate" is expected.", a ruleset violation). */
export function isRequiredCheckRefusal(message: string): boolean {
  return /required status checks?\b/i.test(message)
}

import type { IssuePullRequest } from "./wire"
import type { Tone } from "./issue"

// An issue's pull request in one line: the PR number plus the header pill's
// label the server worded (`IssuePullRequest.status`).

export const NO_PULL_REQUEST = `No PR`

/** The PR that says where the issue stands: its live one (the server lists
 *  newest first), else the newest. */
export function leadingPullRequest(rows: readonly IssuePullRequest[] | null | undefined): IssuePullRequest | null {
  if (!rows?.length) return null
  return rows.find((row) => row.state === `open` || row.state === `draft`) ?? rows[0]!
}

/** `PR #12 · Checks running`. */
export function pullRequestLine(row: IssuePullRequest | null): string {
  return row ? `PR #${row.number} · ${row.status}` : NO_PULL_REQUEST
}

/** The status line: `EXP-12 · PR #12 · Queued · 2nd`, `EXP-12 · No PR`. */
export function issueStatusLine(identifier: string, rows: readonly IssuePullRequest[] | null | undefined): string {
  return `${identifier} · ${pullRequestLine(leadingPullRequest(rows))}`
}

export function pullRequestTone(row: IssuePullRequest): Tone {
  if (row.state === `merged`) return `blue`
  if (row.state === `closed`) return `neutral`
  if (row.checks === `failure`) return `red`
  return row.checks === `success` ? `green` : `neutral`
}

/** What a PR tool's reply says to toast: `PR #12 opened · <url>`. */
export function pullRequestToast(verb: `opened` | `queued`, row: { number?: number; url?: string }): string | null {
  if (typeof row.number !== `number`) return null
  return [`PR #${row.number} ${verb}`, row.url].filter(Boolean).join(` · `)
}

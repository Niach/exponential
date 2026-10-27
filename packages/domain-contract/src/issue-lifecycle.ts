// NEW-1 (plan 3.7): the EXACT issue lifecycle a session drives, ONE rule
// ×4. fixtures/issue-lifecycle.json holds the table + cases; the server
// (`lib/sessions/issue-lifecycle.ts`) is the only writer, the natives
// mirror the table only where they render it.

import fixture from "../fixtures/issue-lifecycle.json" with { type: "json" }

export type IssueLifecycleEvent = `taken` | `pr_opened` | `pr_merged` | `unlinked` | `filed`

export type IssueLifecycleTarget = `backlog` | `in_progress` | `in_review` | `done`

export interface IssueLifecycleState {
  /** The issue's anchor status (contract issueStatus). */
  status: string
  /** A live (open or draft) PR links it. */
  openPr: boolean
  /** Live sessions with a repository that hold it AFTER the event. */
  workers: number
}

/** A session that takes issues: it has a repository (its branch comes at
 *  claim). The fixture names the field. */
export const ISSUE_LIFECYCLE_WORKER_FIELD = fixture.worker

interface Rule {
  event: string
  from: string[]
  to: string
  when?: string
}

const RULES = fixture.rules as Rule[]

/** The anchor status the issue moves to on `event`, or null. */
export function issueLifecycleTarget(
  event: IssueLifecycleEvent,
  state: IssueLifecycleState
): IssueLifecycleTarget | null {
  // `filed` (a session created the issue) holds without taking: never a move.
  if (event === `filed`) return null
  const rule = RULES.find((entry) => entry.event === event)
  if (!rule || !rule.from.includes(state.status)) return null
  if (event === `unlinked` && (state.openPr || state.workers > 0)) return null
  return rule.to as IssueLifecycleTarget
}

export const ISSUE_LIFECYCLE_CASES = fixture.cases as {
  name: string
  input: { event: IssueLifecycleEvent } & IssueLifecycleState
  expected: IssueLifecycleTarget | null
}[]

/** `(n/m)` at the very end of the title. */
const PART_SUFFIX = /\(\s*(\d+)\s*\/\s*(\d+)\s*\)\s*$/

export interface PullRequestPart {
  part: number
  of: number
}

/** EXP-1378: the PR's title ends in `(n/m)`, part n of m (1 <= n <= m,
 *  m >= 2); null = a whole PR. fixtures/issue-lifecycle.json `partPr`. */
export function pullRequestPart(title: string | null | undefined): PullRequestPart | null {
  const match = title?.match(PART_SUFFIX)
  if (!match) return null
  const part = Number(match[1])
  const of = Number(match[2])
  return part >= 1 && of >= 2 && part <= of ? { part, of } : null
}

/** Every part 1..of merged (in any order): the issue lands. */
export function partsLanded(of: number, parts: readonly number[]): boolean {
  const merged = new Set(parts)
  for (let part = 1; part <= of; part++) if (!merged.has(part)) return false
  return true
}

export const PART_PR_CASES = fixture.partPr.cases as {
  name: string
  title: string | null
  expected: PullRequestPart | null
}[]

export const PARTS_LANDED_CASES = fixture.partPr.landedCases as {
  name: string
  of: number
  parts: number[]
  expected: boolean
}[]

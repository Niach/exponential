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

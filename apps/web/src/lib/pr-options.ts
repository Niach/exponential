// Options for a `pr`-typed action input (EXP-259): the team's OPEN
// issue-linked pull requests, deduped by prUrl. A batch coding run links
// several issues to ONE pull request, so a row lists every linked identifier
// and carries the REPRESENTATIVE issue's id as its value — exactly what
// `steer.startSession` resolves server-side (team-scoped, open-state checked).
// Mirrors the native builders (`buildPullRequestOptions` on Android,
// `StartPullRequestOption.build` on iOS) down to the lowest-id representative
// and the label format.

import { workflowFinalPrIdentifier } from "@/lib/workflow-final-pr-identity"

export interface PrOptionIssue {
  id: string
  boardId: string
  identifier: string
  prUrl: string | null
  prNumber: number | null
  prState: string | null
}

/** EXP-1072: a workflow whose ONE final pull request is open — it is the
 *  workflow's own PR, so the `pr` input offers it like any linked PR. */
export interface PrOptionWorkflow {
  id: string
  teamId: string
  name: string
  finalPrNumber: number | null
  finalPrState: string | null
}

export interface PrOption {
  /** The representative issue's id — the value the `pr` input submits.
   *  EXP-1072: a WORKFLOW's id for its final pull request. */
  issueId: string
  prNumber: number | null
  /** Every issue identifier linked to this pull request, sorted. */
  identifiers: string[]
  /** Every linked issue's id — the set `findPrOptionForIssue` matches on. */
  linkedIssueIds: string[]
  /** `#42 · EXP-1, EXP-2` — the PR number when known, then the linked issues. */
  label: string
}

/**
 * Collapse open-PR issue rows into one option per pull request. Rows outside
 * `teamBoardIds`, rows whose PR isn't open, and rows without a `prUrl` are
 * skipped (such an id wouldn't resolve server-side anyway). The representative
 * is the lowest id so it doesn't depend on query order, and the list is sorted
 * by label so it doesn't reshuffle as sync lands rows.
 */
export function buildPrOptions(
  issues: PrOptionIssue[],
  teamBoardIds: Set<string>,
  workflows: PrOptionWorkflow[] = []
): PrOption[] {
  const byPrUrl = new Map<string, PrOption>()
  // EXP-1072: the team's workflows with an OPEN final pull request, one
  // option each, labelled like the PR's title (`#829 · Workflow: EXP-996 +5`).
  for (const workflow of workflows) {
    if (workflow.finalPrState !== `open`) continue
    byPrUrl.set(`workflow:${workflow.id}`, {
      issueId: workflow.id,
      prNumber: workflow.finalPrNumber,
      identifiers: [workflowFinalPrIdentifier(workflow.name)],
      linkedIssueIds: [workflow.id],
      label: ``,
    })
  }
  for (const issue of [...issues].sort((left, right) =>
    left.id.localeCompare(right.id)
  )) {
    if (
      !teamBoardIds.has(issue.boardId) ||
      issue.prState !== `open` ||
      !issue.prUrl
    ) {
      continue
    }
    const entry = byPrUrl.get(issue.prUrl)
    if (entry) {
      if (issue.identifier) entry.identifiers.push(issue.identifier)
      entry.linkedIssueIds.push(issue.id)
    } else {
      byPrUrl.set(issue.prUrl, {
        issueId: issue.id,
        prNumber: issue.prNumber,
        identifiers: issue.identifier ? [issue.identifier] : [],
        linkedIssueIds: [issue.id],
        label: ``,
      })
    }
  }
  return [...byPrUrl.values()]
    .map((entry) => {
      const joined = [...entry.identifiers].sort().join(`, `)
      return {
        ...entry,
        identifiers: [...entry.identifiers].sort(),
        label: entry.prNumber
          ? joined
            ? `#${entry.prNumber} · ${joined}`
            : `#${entry.prNumber}`
          : joined,
      }
    })
    .sort(
      (left, right) =>
        left.label.localeCompare(right.label) ||
        left.issueId.localeCompare(right.issueId)
    )
}

/**
 * The option a given issue id belongs to — by MEMBERSHIP, not by the
 * representative id. A caller seeding the `pr` input holds whatever issue its
 * surface acts on (the Reviews rows pick the NEWEST linked issue, this builder
 * the lowest one), but only the representative id matches an option, so seeds
 * are normalised through this (EXP-323).
 */
export function findPrOptionForIssue(
  options: PrOption[],
  issueId: string
): PrOption | null {
  return (
    options.find((option) => option.linkedIssueIds.includes(issueId)) ?? null
  )
}

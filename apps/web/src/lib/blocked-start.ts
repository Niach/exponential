// SLOP-3: starting a BLOCKED issue asks Cancel · Start anyway · Stacked PR.
// The ONE home of the dialog's copy and its pure rules, byte-identical ×4
// (desktop domain `blocked_start.rs`, iOS `BlockedStart.swift`, Android
// `BlockedStart.kt`), locked by `@exp/domain-contract/fixtures/blocked-start.json`.
//
// A stacked start builds the whole dependency LINE bottom-up and is PROMPT
// TEXT only: the client starts the line's lowest unstarted issue with the base
// instruction and the line paragraph, and the playbook's follow-up rule carries
// it from run to run. No stack plan, no relay frame, no server input.

import { openBlockers, type GraphIssue, type GraphRelation } from "@/lib/issue-graph"

/** The dialog's title for one picked issue. */
export const BLOCKED_START_TITLE = `This issue is blocked`
/** The title and body when two or more issues were picked. */
export const BLOCKED_BATCH_TITLE = `Some of these issues are blocked`
export const BLOCKED_BATCH_BODY = `Open issues outside this batch block it. Start anyway?`
/** The body, around the blocker chips: `<prefix>` chips `<suffix>`. */
export const BLOCKED_START_BODY_PREFIX = `This issue is blocked by `
/** The suffix while "Stacked PR" is disabled. */
export const BLOCKED_START_BODY_SUFFIX = `. Start anyway?`
/** The suffix while "Stacked PR" is enabled. */
export const BLOCKED_START_BODY_SUFFIX_STACKABLE = `. Start anyway, or start a stacked PR?`
/** The secondary button: a plain run, blockers and all. */
export const START_ANYWAY_LABEL = `Start anyway`
/** The primary button: the line, started bottom-up. */
export const STACKED_PR_LABEL = `Stacked PR`

/** The most issues one stacked start may line up (the subject included). */
export const STACK_MAX_RUN = 5

/** Why "Stacked PR" is disabled, in the order `stackPlan` checks them. */
export type StackDisabledReason =
  | `batch`
  | `cycle`
  | `many`
  | `repo`
  | `long`
  | `running`

export const STACK_DISABLED_REASONS: readonly StackDisabledReason[] = [
  `batch`,
  `cycle`,
  `many`,
  `repo`,
  `long`,
  `running`,
]

/** The note under the graph per reason; `{ident}` = the issue it names. */
export const STACK_DISABLED_NOTES: Record<StackDisabledReason, string> = {
  batch: `A stacked PR starts one issue at a time.`,
  cycle: `These issues block each other in a cycle. Remove one relation to stack them.`,
  many: `#{ident} has more than one open blocker. A stacked PR follows one line.`,
  repo: `#{ident} lives in another repository.`,
  long: `A stacked PR starts at most 5 issues in a line.`,
  running: `#{ident} is already running. Its pull request is not open yet.`,
}

/** The note under the graph while the button is enabled and the run has 2+
 *  issues. */
export const STACK_PLAN_NOTE_TEMPLATE = `Starts #{first} first, then #{rest}.`
/** The base instruction, when the line has an open PR to stack on. */
export const STACK_BASE_TEMPLATE =
  `Stacked on #{ident}. Before any edit: \`git fetch origin {branch}\`; ` +
  `if this branch has no commits of its own, \`git reset --hard origin/{branch}\`, ` +
  `else \`git rebase origin/{branch}\`. ` +
  `Open your PR with \`exponential_pr_open({issueId, base: "{branch}"})\`.`
/** The line paragraph, when the run has 2+ issues. */
export const STACK_LINE_TEMPLATE =
  `This is the bottom of a stacked line: {line}. After your PR is open and ` +
  `your branch is pushed, start #{next} as a follow-up run on your branch ` +
  `(playbook: Follow-ups and follow-up runs), even under \`no follow-up runs\`. ` +
  `If more issues follow it, give it this paragraph with your own issue ` +
  `dropped from the line. Do not wait for it.`
/** The typed text of a run of 2+, addressed to the subject. */
export const STACK_TEXT_TEMPLATE = `Instructions for #{subject}, pass them along unchanged:\n{text}`

// ── (1) The line ────────────────────────────────────────────────────────────

export interface StackLine<T> {
  /** The open issues BELOW the subject, BOTTOM first. */
  line: T[]
  /** The identifier of the issue with two or more open blockers, if any. */
  fork: string | null
  /** The walk met an issue twice. */
  cycle: boolean
}

/**
 * Walk DOWN from the subject over its open direct blockers (`openBlockers`):
 * none stops, exactly one joins the line and the walk continues from it, more
 * than one is a fork (named by the issue that has them), an issue seen twice
 * is a cycle.
 */
export function stackLine<T extends GraphIssue>(
  subjectId: string,
  relations: readonly GraphRelation[],
  issues: readonly T[]
): StackLine<T> {
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  const seen = new Set<string>([subjectId])
  const above: T[] = []
  let fork: string | null = null
  let cycle = false
  let currentId = subjectId
  for (;;) {
    const blockers = openBlockers(currentId, relations, issues)
    if (blockers.length === 0) break
    if (blockers.length > 1) {
      fork = byId.get(currentId)?.identifier ?? null
      break
    }
    const next = blockers[0]!
    if (seen.has(next.id)) {
      cycle = true
      break
    }
    seen.add(next.id)
    above.push(next)
    currentId = next.id
  }
  return { line: above.reverse(), fork, cycle }
}

// ── (2) The plan ────────────────────────────────────────────────────────────

/** A line member as the plan reads it. */
export interface StackLineMember {
  identifier: string
  prState: string | null
  branch: string | null
  repositoryId: string | null
  /** A LIVE run is on it. */
  running: boolean
}

export interface StackPlan {
  /** The open PR the first run stacks on; null = the default branch. */
  base: { identifier: string; branch: string } | null
  /** The issues to start, bottom first, ending with the subject. */
  run: string[]
}

export interface StackPlanResult {
  plan: StackPlan | null
  reason: StackDisabledReason | null
  /** The issue the reason's note names (`many`, `repo`, `running`). */
  ident: string | null
}

export function stackPlan(args: {
  pickedCount: number
  subject: { identifier: string; repositoryId: string | null }
  line: readonly StackLineMember[]
  fork: string | null
  cycle: boolean
}): StackPlanResult {
  const { subject, line } = args
  const off = (
    reason: StackDisabledReason,
    ident: string | null = null
  ): StackPlanResult => ({ plan: null, reason, ident })

  if (args.pickedCount > 1) return off(`batch`)
  if (args.cycle) return off(`cycle`)
  if (args.fork !== null) return off(`many`, args.fork)

  const foreign = subject.repositoryId
    ? line.find((member) => member.repositoryId !== subject.repositoryId)
    : line[0]
  if (foreign) return off(`repo`, foreign.identifier)
  if (!subject.repositoryId) return off(`repo`, subject.identifier)

  let baseIndex = -1
  for (let at = line.length - 1; at >= 0; at -= 1) {
    const member = line[at]!
    if (member.prState === `open` && member.branch) {
      baseIndex = at
      break
    }
  }
  const above = line.slice(baseIndex + 1)
  const run = [...above.map((member) => member.identifier), subject.identifier]
  if (run.length > STACK_MAX_RUN) return off(`long`)
  const busy = above.find((member) => member.running)
  if (busy) return off(`running`, busy.identifier)

  const baseMember = baseIndex >= 0 ? line[baseIndex]! : null
  return {
    plan: {
      base: baseMember
        ? { identifier: baseMember.identifier, branch: baseMember.branch! }
        : null,
      run,
    },
    reason: null,
    ident: null,
  }
}

// ── Copy ────────────────────────────────────────────────────────────────────

/** The note under the graph while "Stacked PR" is disabled; `ident` = the
 *  issue the reason names (ignored by the notes that name none). */
export function stackDisabledNote(
  reason: StackDisabledReason,
  ident: string
): string {
  return STACK_DISABLED_NOTES[reason].replaceAll(`{ident}`, ident)
}

/** The note under the graph while the button is enabled; null for a run of
 *  one (nothing to announce). */
export function stackPlanNote(run: readonly string[]): string | null {
  if (run.length < 2) return null
  return STACK_PLAN_NOTE_TEMPLATE.replace(`{first}`, run[0]!).replace(
    `{rest}`,
    run.slice(1).join(`, then #`)
  )
}

/** The prompt the start of `run[0]` sends: the base instruction, the line
 *  paragraph and the typed text, joined by a blank line. */
export function stackedStartPrompt(plan: StackPlan, text: string): string {
  const parts: string[] = []
  if (plan.base) {
    parts.push(
      STACK_BASE_TEMPLATE.replaceAll(`{ident}`, plan.base.identifier).replaceAll(
        `{branch}`,
        plan.base.branch
      )
    )
  }
  if (plan.run.length >= 2) {
    parts.push(
      STACK_LINE_TEMPLATE.replace(
        `{line}`,
        plan.run.map((ident) => `#${ident}`).join(`, then `)
      ).replace(`{next}`, plan.run[1]!)
    )
  }
  const typed = text.trim()
  if (typed.length > 0) {
    parts.push(
      plan.run.length < 2
        ? typed
        : STACK_TEXT_TEMPLATE.replace(
            `{subject}`,
            plan.run[plan.run.length - 1]!
          ).replace(`{text}`, () => typed)
    )
  }
  return parts.join(`\n\n`)
}

// SLOP-3: starting a BLOCKED issue asks Cancel · Start anyway · Stacked PR.
// The ONE home of the dialog's copy and its two pure rules, byte-identical ×4
// (desktop domain `blocked_start.rs`, iOS `BlockedStart.swift`, Android
// `BlockedStart.kt`), locked by `@exp/domain-contract/fixtures/blocked-start.json`.
//
// A stacked start is PROMPT TEXT only: the same base instruction a follow-up
// run gets from the playbook (fetch the blocker's branch, reset or rebase on
// it, open the PR with `base`). No stack plan, no relay frame, no server input.

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
/** The primary button: the same run, stacked on the one blocker's PR. */
export const STACKED_PR_LABEL = `Stacked PR`

/** Why "Stacked PR" is disabled, in the order `stackTarget` checks them. */
export type StackDisabledReason = `batch` | `many` | `no-pr` | `repo`

export const STACK_DISABLED_REASONS: readonly StackDisabledReason[] = [
  `batch`,
  `many`,
  `no-pr`,
  `repo`,
]

/** A blocker as the rule reads it: its PR state and branch, and the
 *  repository of its board. */
export interface StackBlocker {
  identifier: string
  prState: string | null
  branch: string | null
  repositoryId: string | null
}

export interface StackTarget<B extends StackBlocker> {
  /** The blocker to stack on; null whenever `reason` is set. */
  target: B | null
  reason: StackDisabledReason | null
}

/**
 * The one blocker a stacked start bases on, or the one reason it cannot.
 * First match wins: a batch never stacks, two or more open blockers do not,
 * the blocker needs an OPEN pull request with a recorded branch, and both
 * ends must live in the same (non-null) repository.
 */
export function stackTarget<B extends StackBlocker>(args: {
  pickedCount: number
  subjectRepositoryId: string | null
  blockers: readonly B[]
}): StackTarget<B> {
  const off = (reason: StackDisabledReason): StackTarget<B> => ({
    target: null,
    reason,
  })
  if (args.pickedCount > 1) return off(`batch`)
  if (args.blockers.length !== 1) return off(`many`)
  const blocker = args.blockers[0]!
  if (blocker.prState !== `open` || !blocker.branch) return off(`no-pr`)
  if (
    !args.subjectRepositoryId ||
    blocker.repositoryId !== args.subjectRepositoryId
  ) {
    return off(`repo`)
  }
  return { target: blocker, reason: null }
}

/** The note under the graph while "Stacked PR" is disabled; `ident` = the
 *  one blocker's identifier (the `no-pr` and `repo` notes name it). */
export function stackDisabledNote(
  reason: StackDisabledReason,
  ident: string
): string {
  switch (reason) {
    case `batch`:
      return `A stacked PR starts one issue at a time.`
    case `many`:
      return `A stacked PR needs exactly one open blocker.`
    case `no-pr`:
      return `#${ident} has no open pull request yet.`
    case `repo`:
      return `#${ident} lives in another repository.`
  }
}

/** The prompt a stacked start sends: the base instruction, then the typed
 *  text (trimmed) after a blank line when there is any. */
export function stackedStartPrompt(
  identifier: string,
  branch: string,
  text: string
): string {
  const base =
    `Stacked on #${identifier}. Before any edit: \`git fetch origin ${branch}\`; ` +
    `if this branch has no commits of its own, \`git reset --hard origin/${branch}\`, ` +
    `else \`git rebase origin/${branch}\`. ` +
    `Open your PR with \`exponential_pr_open({issueId, base: "${branch}"})\`.`
  const typed = text.trim()
  return typed.length > 0 ? `${base}\n\n${typed}` : base
}

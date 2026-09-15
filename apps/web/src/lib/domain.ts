import type { IssuePriority, IssueStatus } from "@exp/db-schema/domain"
import { BUILTIN_STATUS_COLOR_CLASS, conceptIcon } from "@exp/ui"

export * from "@exp/db-schema/domain"

// EXP-887: the interface itself lives in @exp/ui (the menu primitive that
// consumes it does), re-exported here so the option tables below and their
// ~30 call sites keep naming it from one place.
import type { IssueOption } from "@exp/ui"
export type { IssueOption }

// Option tables — the ONE picker vocabulary (REV2-85): every status/priority
// menu on every client walks the contract `displayOrder`
// (packages/domain-contract/contract.json), the same order the board groups
// and the statuses settings page use (EXP-448), so picker muscle memory
// transfers between web, desktop, iOS and Android. Locked against the
// contract by lib/domain-contract.test.ts.
//
// EXP-314: `issueStatusOptions` / `getIssueStatusConfig` are now the ANCHOR
// FALLBACK layer, not the status vocabulary. Status UI resolves per-team rows
// through `useTeamStatuses` (hooks/use-team-statuses.tsx); these tables supply
// (a) the LEGACY token colors builtin rows still render with, (b) the enum
// fallback for legacy URL tokens / old timeline payloads, and (c) anchor-keyed
// logic (`CODEABLE_STATUSES` and friends). New UI must not group or pick from
// them directly.
// EXP-887: the `color` strings live in @exp/ui's `status-icons.ts` — Tailwind
// v4 only emits a palette utility it can SEE in a scanned file, and a class
// named only here would stop being generated once the last package-side
// consumer needed it. The PRIORITY palette stays below: nothing in the package
// renders a priority.
export const issueStatusOptions = [
  {
    value: `backlog`,
    label: `Backlog`,
    icon: conceptIcon(`status-backlog`),
    color: BUILTIN_STATUS_COLOR_CLASS.backlog,
  },
  {
    value: `in_progress`,
    label: `In Progress`,
    icon: conceptIcon(`status-in-progress`),
    color: BUILTIN_STATUS_COLOR_CLASS.in_progress,
  },
  {
    value: `in_review`,
    label: `In Review`,
    icon: conceptIcon(`status-in-review`),
    color: BUILTIN_STATUS_COLOR_CLASS.in_review,
  },
  {
    value: `done`,
    label: `Done`,
    icon: conceptIcon(`status-done`),
    color: BUILTIN_STATUS_COLOR_CLASS.done,
  },
  {
    value: `cancelled`,
    label: `Cancelled`,
    icon: conceptIcon(`status-cancelled`),
    color: BUILTIN_STATUS_COLOR_CLASS.cancelled,
  },
  {
    value: `duplicate`,
    label: `Duplicate`,
    icon: conceptIcon(`status-duplicate`),
    color: BUILTIN_STATUS_COLOR_CLASS.duplicate,
  },
] as const satisfies readonly IssueOption<IssueStatus>[]

export const issuePriorityOptions = [
  {
    value: `urgent`,
    label: `Urgent`,
    icon: conceptIcon(`priority-urgent`),
    color: `text-red-500`,
  },
  {
    value: `high`,
    label: `High`,
    icon: conceptIcon(`priority-high`),
    color: `text-orange-500`,
  },
  {
    value: `medium`,
    label: `Medium`,
    icon: conceptIcon(`priority-medium`),
    color: `text-yellow-500`,
  },
  {
    value: `low`,
    label: `Low`,
    icon: conceptIcon(`priority-low`),
    color: `text-blue-500`,
  },
  {
    value: `none`,
    label: `No priority`,
    icon: conceptIcon(`priority-none`),
    color: `text-muted-foreground`,
  },
] as const satisfies readonly IssueOption<IssuePriority>[]

function getOptionConfig<TValue extends string>(
  options: readonly IssueOption<TValue>[],
  value: TValue | string,
  fallback: IssueOption<TValue>
): IssueOption<TValue> {
  return options.find((option) => option.value === value) ?? fallback
}

// Unknown/forward-compat wire values fall back to the lifecycle START of the
// vocabulary (backlog / no priority), NOT to the first row of the display
// order — the tables are display-ordered, so the fallbacks are looked up by
// value. Anything that resolves an option out of these tables (the config
// helpers below, `OptionDropdownMenu`'s trigger) must use these constants.
export const ISSUE_STATUS_FALLBACK = `backlog` satisfies IssueStatus
export const ISSUE_PRIORITY_FALLBACK = `none` satisfies IssuePriority

const backlogOption = issueStatusOptions.find(
  (option) => option.value === ISSUE_STATUS_FALLBACK
)!
const noPriorityOption = issuePriorityOptions.find(
  (option) => option.value === ISSUE_PRIORITY_FALLBACK
)!

export function getIssueStatusConfig(status: IssueStatus | string) {
  return getOptionConfig(issueStatusOptions, status, backlogOption)
}

export function getIssuePriorityConfig(priority: IssuePriority | string) {
  return getOptionConfig(issuePriorityOptions, priority, noPriorityOption)
}

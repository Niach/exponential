import type { LucideIcon } from "lucide-react"
import {
  AlertTriangle,
  Circle,
  CircleCheck,
  CircleDashed,
  CircleX,
  Copy,
  GitPullRequest,
  Minus,
  SignalHigh,
  SignalLow,
  SignalMedium,
  Timer,
} from "lucide-react"
import type { IssuePriority, IssueStatus } from "@exp/db-schema/domain"

export * from "@exp/db-schema/domain"

export interface IssueOption<TValue extends string> {
  color: string
  icon: LucideIcon
  label: string
  value: TValue
}

// Option tables — the ONE picker vocabulary (REV2-85): every status/priority
// menu on every client walks the contract `displayOrder`
// (packages/domain-contract/contract.json), the same order the board groups
// use, so picker muscle memory transfers between web, desktop, iOS and
// Android. Locked against the contract by lib/domain-contract.test.ts.
export const issueStatusOptions = [
  {
    value: `in_progress`,
    label: `In Progress`,
    icon: Timer,
    color: `text-yellow-500`,
  },
  {
    value: `in_review`,
    label: `In Review`,
    icon: GitPullRequest,
    color: `text-green-500`,
  },
  {
    value: `todo`,
    label: `Todo`,
    icon: Circle,
    color: `text-foreground`,
  },
  {
    value: `backlog`,
    label: `Backlog`,
    icon: CircleDashed,
    color: `text-muted-foreground`,
  },
  {
    value: `done`,
    label: `Done`,
    icon: CircleCheck,
    color: `text-blue-500`,
  },
  {
    value: `cancelled`,
    label: `Cancelled`,
    icon: CircleX,
    color: `text-muted-foreground`,
  },
  {
    value: `duplicate`,
    label: `Duplicate`,
    icon: Copy,
    color: `text-muted-foreground`,
  },
] as const satisfies readonly IssueOption<IssueStatus>[]

export const issuePriorityOptions = [
  {
    value: `urgent`,
    label: `Urgent`,
    icon: AlertTriangle,
    color: `text-red-500`,
  },
  {
    value: `high`,
    label: `High`,
    icon: SignalHigh,
    color: `text-orange-500`,
  },
  {
    value: `medium`,
    label: `Medium`,
    icon: SignalMedium,
    color: `text-yellow-500`,
  },
  {
    value: `low`,
    label: `Low`,
    icon: SignalLow,
    color: `text-blue-500`,
  },
  {
    value: `none`,
    label: `No priority`,
    icon: Minus,
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

import type { LucideIcon } from "lucide-react"
import {
  AlertTriangle,
  Circle,
  CircleCheck,
  CircleDashed,
  CircleX,
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

export const issueStatusOptions = [
  {
    value: `backlog`,
    label: `Backlog`,
    icon: CircleDashed,
    color: `text-muted-foreground`,
  },
  {
    value: `todo`,
    label: `Todo`,
    icon: Circle,
    color: `text-foreground`,
  },
  {
    value: `in_progress`,
    label: `In Progress`,
    icon: Timer,
    color: `text-yellow-500`,
  },
  {
    value: `done`,
    label: `Done`,
    icon: CircleCheck,
    color: `text-green-500`,
  },
  {
    value: `cancelled`,
    label: `Cancelled`,
    icon: CircleX,
    color: `text-muted-foreground`,
  },
] as const satisfies readonly IssueOption<IssueStatus>[]

export const issuePriorityOptions = [
  {
    value: `none`,
    label: `No priority`,
    icon: Minus,
    color: `text-muted-foreground`,
  },
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
] as const satisfies readonly IssueOption<IssuePriority>[]

function getOptionConfig<TValue extends string>(
  options: readonly IssueOption<TValue>[],
  value: TValue | string,
  fallback: IssueOption<TValue>
): IssueOption<TValue> {
  return options.find((option) => option.value === value) ?? fallback
}

export function getIssueStatusConfig(status: IssueStatus | string) {
  return getOptionConfig(issueStatusOptions, status, issueStatusOptions[0])
}

export function getIssuePriorityConfig(priority: IssuePriority | string) {
  return getOptionConfig(
    issuePriorityOptions,
    priority,
    issuePriorityOptions[0]
  )
}

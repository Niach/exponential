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
import { z } from "zod"

export const issueStatusValues = [
  `backlog`,
  `todo`,
  `in_progress`,
  `done`,
  `cancelled`,
] as const

export const issuePriorityValues = [
  `none`,
  `urgent`,
  `high`,
  `medium`,
  `low`,
] as const

export const workspaceRoleValues = [`owner`, `member`] as const

export type IssueStatus = (typeof issueStatusValues)[number]
export type IssuePriority = (typeof issuePriorityValues)[number]
export type WorkspaceRole = (typeof workspaceRoleValues)[number]

export const issueStatusSchema = z.enum(issueStatusValues)
export const issuePrioritySchema = z.enum(issuePriorityValues)
export const workspaceRoleSchema = z.enum(workspaceRoleValues)
export const dateOnlySchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)

export const issueDescriptionSchema = z.object({
  text: z.string(),
})

export type IssueDescription = z.infer<typeof issueDescriptionSchema>

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

export const issueStatusOrder: IssueStatus[] = [
  `in_progress`,
  `todo`,
  `backlog`,
  `done`,
  `cancelled`,
]

function getOptionConfig<TValue extends string>(
  options: readonly IssueOption<TValue>[],
  value: TValue | string,
  fallback: IssueOption<TValue>
): IssueOption<TValue> {
  return (
    options.find((option) => option.value === value) ??
    fallback
  )
}

export function getIssueStatusConfig(status: IssueStatus | string) {
  return getOptionConfig(issueStatusOptions, status, issueStatusOptions[0])
}

export function getIssuePriorityConfig(priority: IssuePriority | string) {
  return getOptionConfig(issuePriorityOptions, priority, issuePriorityOptions[0])
}

export function getIssueDescriptionText(description: unknown): string {
  const parsed = issueDescriptionSchema.safeParse(description)
  return parsed.success ? parsed.data.text : ``
}

export function toIssueDescription(
  text: string
): IssueDescription | null {
  const trimmed = text.trim()
  return trimmed ? { text: trimmed } : null
}

export function formatDateForMutation(date: Date | null | undefined) {
  return date ? date.toISOString().split(`T`)[0] : null
}

import { addDays, addMonths, addWeeks } from "date-fns"
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

export const workspaceRoleValues = [`owner`, `member`, `agent`] as const

export const publicWritePolicyValues = [`members`, `everyone`] as const

export const recurrenceUnitValues = [`day`, `week`, `month`] as const

// Selectable recurrence interval options shown in the editor. Mirrors
// packages/domain-contract/contract.json (kept in sync by the domain-contract
// drift test in apps/web).
export const recurrenceIntervals = [
  1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 21, 30,
] as const

export const commentKindValues = [`regular`, `question`, `plan`] as const

export const agentPlanStateValues = [
  `drafting`,
  `awaiting_approval`,
  `awaiting_answer`,
  `approved`,
] as const

export type IssueStatus = (typeof issueStatusValues)[number]
export type IssuePriority = (typeof issuePriorityValues)[number]
export type WorkspaceRole = (typeof workspaceRoleValues)[number]
export type PublicWritePolicy = (typeof publicWritePolicyValues)[number]
export type RecurrenceUnit = (typeof recurrenceUnitValues)[number]
export type CommentKind = (typeof commentKindValues)[number]
export type AgentPlanState = (typeof agentPlanStateValues)[number]

export const issueStatusSchema = z.enum(issueStatusValues)
export const issuePrioritySchema = z.enum(issuePriorityValues)
export const workspaceRoleSchema = z.enum(workspaceRoleValues)
export const publicWritePolicySchema = z.enum(publicWritePolicyValues)
export const recurrenceUnitSchema = z.enum(recurrenceUnitValues)
export const commentKindSchema = z.enum(commentKindValues)
export const agentPlanStateSchema = z.enum(agentPlanStateValues)
export const recurrenceIntervalSchema = z.number().int().min(1).max(999)
export const dateOnlySchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
export const timeOnlySchema = z.string().regex(/^\d{2}:\d{2}(:\d{2})?$/)

export const issueDescriptionSchema = z.object({
  text: z.string(),
})

export type IssueDescription = z.infer<typeof issueDescriptionSchema>

export const commentBodySchema = z.object({
  text: z.string().min(1).max(10_000),
})

export type CommentBody = z.infer<typeof commentBodySchema>

export function getCommentBodyText(body: unknown): string {
  const parsed = commentBodySchema.safeParse(body)
  return parsed.success ? parsed.data.text : ``
}

export const issueStatusOrder: IssueStatus[] = [
  `in_progress`,
  `todo`,
  `backlog`,
  `done`,
  `cancelled`,
]

export function getIssueDescriptionText(description: unknown): string {
  const parsed = issueDescriptionSchema.safeParse(description)
  return parsed.success ? parsed.data.text : ``
}

export function normalizeIssueDescriptionText(text: string) {
  return text.trim()
}

export function toIssueDescription(text: string): IssueDescription | null {
  const trimmed = normalizeIssueDescriptionText(text)
  return trimmed ? { text: trimmed } : null
}

export function addRecurrence(
  date: Date,
  interval: number,
  unit: RecurrenceUnit
): Date {
  switch (unit) {
    case `day`:
      return addDays(date, interval)
    case `week`:
      return addWeeks(date, interval)
    case `month`:
      return addMonths(date, interval)
  }
}

export function formatRecurrence(interval: number, unit: RecurrenceUnit) {
  const noun = interval === 1 ? unit : `${unit}s`
  return interval === 1 ? `Every ${noun}` : `Every ${interval} ${noun}`
}

export function formatDateForMutation(date: Date | null | undefined) {
  if (!date) {
    return null
  }

  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, `0`)
  const day = String(date.getDate()).padStart(2, `0`)

  return `${year}-${month}-${day}`
}

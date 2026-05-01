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

export const workspaceRoleValues = [`owner`, `member`] as const

export const recurrenceUnitValues = [`day`, `week`, `month`] as const

export type IssueStatus = (typeof issueStatusValues)[number]
export type IssuePriority = (typeof issuePriorityValues)[number]
export type WorkspaceRole = (typeof workspaceRoleValues)[number]
export type RecurrenceUnit = (typeof recurrenceUnitValues)[number]

export const issueStatusSchema = z.enum(issueStatusValues)
export const issuePrioritySchema = z.enum(issuePriorityValues)
export const workspaceRoleSchema = z.enum(workspaceRoleValues)
export const recurrenceUnitSchema = z.enum(recurrenceUnitValues)
export const recurrenceIntervalSchema = z.number().int().min(1).max(999)
export const dateOnlySchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
export const timeOnlySchema = z.string().regex(/^\d{2}:\d{2}(:\d{2})?$/)

export const issueDescriptionSchema = z.object({
  text: z.string(),
})

export type IssueDescription = z.infer<typeof issueDescriptionSchema>

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

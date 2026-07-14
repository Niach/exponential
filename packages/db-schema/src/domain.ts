import { addDays, addMonths, addWeeks } from "date-fns"
import { z } from "zod"

export const issueStatusValues = [
  `backlog`,
  `todo`,
  `in_progress`,
  `done`,
  `cancelled`,
  // Terminal resolution: this issue is a duplicate of `issues.duplicateOfId`.
  // Hidden from active lists like done/cancelled.
  `duplicate`,
] as const

export const issuePriorityValues = [
  `none`,
  `urgent`,
  `high`,
  `medium`,
  `low`,
] as const

export const workspaceRoleValues = [`owner`, `member`] as const

// What a project IS (projects.type). `dev` is repo-backed (repository required,
// coding sessions/PRs); `tasks` is plain issue tracking with no repo; `feedback`
// is a PUBLIC read-only board (anonymous browsing, writes only via the embedded
// widget) with an optional repo. Coding features gate on repo presence, not type.
export const projectTypeValues = [`dev`, `tasks`, `feedback`] as const

// How long a soft-deleted (trashed) project is retained before the purge sweep
// hard-deletes it (with all its issues) and reclaims its attachment storage.
// The single source every client mirrors for the restore-window countdown; the
// purge time is computed as deletedAt + this, never stored.
export const PROJECT_TRASH_RETENTION_HOURS = 48
export const PROJECT_TRASH_RETENTION_MS =
  PROJECT_TRASH_RETENTION_HOURS * 60 * 60 * 1000

// How long a `running` coding_sessions row may go without a liveness signal
// (updated_at — the desktop heartbeats it while the claude child is alive)
// before the server-side staleness sweep DELETES it. The desktop's exit hook
// is the normal end path, but it is in-process only — a SIGKILL/panic/power
// loss never fires it, and nothing reconciles on relaunch. The sweep deletes
// rather than flipping to `ended` because the desktop's own-row kill-switch
// treats that flip as a remote kill of the live claude child, while a
// vanished row deliberately never fires it — so even a live session whose
// heartbeats all fail (or a pre-heartbeat desktop build) only loses its
// badge, never its process. Generous relative to the heartbeat cadence so
// flaky pings can never strand a live session's badge.
export const CODING_SESSION_STALE_HOURS = 24
export const CODING_SESSION_STALE_MS =
  CODING_SESSION_STALE_HOURS * 60 * 60 * 1000

export const recurrenceUnitValues = [`day`, `week`, `month`] as const

// Selectable recurrence interval options shown in the editor. Mirrors
// packages/domain-contract/contract.json (kept in sync by the domain-contract
// drift test in apps/web).
export const recurrenceIntervals = [
  1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 21, 30,
] as const

// Only `regular` (human) comments exist.
export const commentKindValues = [`regular`] as const

// Notification kinds. Mirrors the `notification_type` pg enum in schema.ts;
// promoted into the contract so the native inbox can label rows.
export const notificationTypeValues = [
  `issue_assigned`,
  `issue_comment`,
  `issue_status_changed`,
  `issue_mention`,
  // New-issue broadcast to workspace members — currently fired only for
  // feedback-widget submissions (external reporters have no other signal path).
  `issue_created`,
  // PR lifecycle notifications — fan out to assignee + subscribers so the
  // away/phone flow gets "PR opened" and "it's merged" on every channel.
  `pr_opened`,
  `pr_merged`,
] as const

// Pull-request state surfaced on issues.pr_state. Mirrors the GitHub PR state
// machine (written by the MCP open_pr tool + the merge webhook/cron).
export const prStateValues = [`open`, `closed`, `merged`, `draft`] as const

// Lifecycle of a live desktop coding session (coding_sessions.status). A row
// is one interactive terminal session (one ghostty + one claude child in one
// worktree); `running` drives the "coding now" badge + Watch/Steer button.
export const codingSessionStatusValues = [`running`, `ended`] as const

// Why a user is subscribed to an issue (issue_subscribers.source, varchar).
// `manual` records an explicit (un)subscribe and suppresses auto-resubscribe.
// `widget_reporter` rows model an external feedback-widget reporter: null
// userId, `email` set — they receive the one-way resolution email on close.
export const subscriberSourceValues = [
  `creator`,
  `assignee`,
  `commenter`,
  `manual`,
  `mention`,
  `widget_reporter`,
] as const

// Activity-log event kinds (issue_events.type, varchar). Drives the
// Linear-style timeline on every client.
export const issueEventTypeValues = [
  `status_changed`,
  `assignee_changed`,
  `label_added`,
  `label_removed`,
  `pr_opened`,
  `pr_merged`,
  `release_added`,
  `release_removed`,
  `project_moved`,
] as const

export type IssueStatus = (typeof issueStatusValues)[number]
export type IssuePriority = (typeof issuePriorityValues)[number]
export type WorkspaceRole = (typeof workspaceRoleValues)[number]
export type ProjectType = (typeof projectTypeValues)[number]
export type RecurrenceUnit = (typeof recurrenceUnitValues)[number]
export type CommentKind = (typeof commentKindValues)[number]
export type NotificationType = (typeof notificationTypeValues)[number]
export type PrState = (typeof prStateValues)[number]
export type CodingSessionStatus = (typeof codingSessionStatusValues)[number]
export type SubscriberSource = (typeof subscriberSourceValues)[number]
export type IssueEventType = (typeof issueEventTypeValues)[number]

export const issueStatusSchema = z.enum(issueStatusValues)
export const issuePrioritySchema = z.enum(issuePriorityValues)
export const workspaceRoleSchema = z.enum(workspaceRoleValues)
export const projectTypeSchema = z.enum(projectTypeValues)
export const recurrenceUnitSchema = z.enum(recurrenceUnitValues)
export const commentKindSchema = z.enum(commentKindValues)
export const notificationTypeSchema = z.enum(notificationTypeValues)
export const prStateSchema = z.enum(prStateValues)
export const codingSessionStatusSchema = z.enum(codingSessionStatusValues)
export const subscriberSourceSchema = z.enum(subscriberSourceValues)
export const issueEventTypeSchema = z.enum(issueEventTypeValues)
export const recurrenceIntervalSchema = z.number().int().min(1).max(999)
export const dateOnlySchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
export const timeOnlySchema = z.string().regex(/^\d{2}:\d{2}(:\d{2})?$/)

// Issue descriptions and comment bodies are plain GFM markdown strings (stored
// in `text` columns). The legacy jsonb `{ text }` envelope was unwrapped; the
// helpers below stay tolerant of any old `{ text }` rows still in flight.
export const issueDescriptionSchema = z.string()

export type IssueDescription = z.infer<typeof issueDescriptionSchema>

export const commentBodySchema = z.string().min(1).max(10_000)

export type CommentBody = z.infer<typeof commentBodySchema>

export function getCommentBodyText(body: unknown): string {
  if (typeof body === `string`) return body
  if (body && typeof body === `object` && `text` in body) {
    const t = (body as { text?: unknown }).text
    return typeof t === `string` ? t : ``
  }
  return ``
}

export const issueStatusOrder: IssueStatus[] = [
  `in_progress`,
  `todo`,
  `backlog`,
  `done`,
  `cancelled`,
  `duplicate`,
]

export function getIssueDescriptionText(description: unknown): string {
  if (typeof description === `string`) return description
  if (description && typeof description === `object` && `text` in description) {
    const t = (description as { text?: unknown }).text
    return typeof t === `string` ? t : ``
  }
  return ``
}

export function normalizeIssueDescriptionText(text: string) {
  return text.trim()
}

export function toIssueDescription(text: string): string | null {
  const trimmed = normalizeIssueDescriptionText(text)
  return trimmed ? trimmed : null
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

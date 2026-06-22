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

// Only `regular` (human) comments exist now. Agent plan/question lifecycle moved
// off comments into the structured issue_agent_state store; the legacy `plan` /
// `question` kinds were drained (their rows deleted by migration 0012). Old rows,
// if any survive, decode tolerantly on clients (unknown kind → regular).
export const commentKindValues = [`regular`] as const

// The agent plan/run lifecycle. The first four are the server-synced states
// (issues.agent_plan_state); the latter four are agent-only progress states the
// native clients render as badges (kept here so badge logic doesn't fall
// through to a default). Stored as varchar — not a pg enum.
export const agentPlanStateValues = [
  `drafting`,
  `awaiting_approval`,
  `awaiting_answer`,
  `approved`,
  `coding`,
  `planning`,
  `in_review`,
  `pushed`,
] as const

// Notification kinds. Mirrors the `notification_type` pg enum in schema.ts;
// promoted into the contract so the native inbox can label rows.
export const notificationTypeValues = [
  `issue_assigned`,
  `issue_comment`,
  `issue_status_changed`,
  `issue_mention`,
  // Action-needed agent notifications (the only agent events that push): a
  // plan is ready to approve, or the agent is waiting on an answer.
  `agent_plan_review`,
  `agent_question`,
] as const

// Pull-request state surfaced on issues.pr_state (varchar). Mirrors the GitHub
// PR state machine the agent reports back.
export const prStateValues = [`open`, `closed`, `merged`, `draft`] as const

// How an agent run was launched (issues.agent_run_mode, varchar).
export const runModeValues = [`background`, `interactive`] as const

// Device/build platform for a project preview run target. Selects the embedding
// backend in the desktop apps (web webview / android emulator / ios simulator).
// Mirrors packages/domain-contract/contract.json (kept in sync by the
// domain-contract drift test in apps/web).
export const platformValues = [`web`, `android`, `ios`] as const

// Why a user is subscribed to an issue (issue_subscribers.source, varchar).
// `manual` records an explicit (un)subscribe and suppresses auto-resubscribe.
export const subscriberSourceValues = [
  `creator`,
  `assignee`,
  `commenter`,
  `manual`,
  `mention`,
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
  `plan_ready`,
  `agent_error`,
  // Agent activity-feed events (decoupled from comments): the agent began
  // working, asked a question, and the human answered it.
  `agent_started`,
  `agent_question`,
  `agent_answer`,
] as const

export type IssueStatus = (typeof issueStatusValues)[number]
export type IssuePriority = (typeof issuePriorityValues)[number]
export type WorkspaceRole = (typeof workspaceRoleValues)[number]
export type PublicWritePolicy = (typeof publicWritePolicyValues)[number]
export type RecurrenceUnit = (typeof recurrenceUnitValues)[number]
export type CommentKind = (typeof commentKindValues)[number]
export type AgentPlanState = (typeof agentPlanStateValues)[number]
export type NotificationType = (typeof notificationTypeValues)[number]
export type PrState = (typeof prStateValues)[number]
export type RunMode = (typeof runModeValues)[number]
export type Platform = (typeof platformValues)[number]
export type SubscriberSource = (typeof subscriberSourceValues)[number]
export type IssueEventType = (typeof issueEventTypeValues)[number]

export const issueStatusSchema = z.enum(issueStatusValues)
export const issuePrioritySchema = z.enum(issuePriorityValues)
export const workspaceRoleSchema = z.enum(workspaceRoleValues)
export const publicWritePolicySchema = z.enum(publicWritePolicyValues)
export const recurrenceUnitSchema = z.enum(recurrenceUnitValues)
export const commentKindSchema = z.enum(commentKindValues)
export const agentPlanStateSchema = z.enum(agentPlanStateValues)
export const notificationTypeSchema = z.enum(notificationTypeValues)
export const prStateSchema = z.enum(prStateValues)
export const runModeSchema = z.enum(runModeValues)
export const platformSchema = z.enum(platformValues)
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

// ---------------------------------------------------------------------------
// Project preview / run targets
// ---------------------------------------------------------------------------
//
// A project's preview config is split across two stores with very different
// trust levels:
//
//   1. The committed repo file `.exponential/config.json` (`ProjectPreviewConfig`)
//      is the CANONICAL source for the build/run *shell commands*. It travels
//      with the repo, is agent-editable, and is read ONLY from the cloned
//      working tree by the desktop apps — never auto-run from a synced DB value.
//   2. The `projects.preview_config` DB column (`ProjectPreviewMirror`) is a
//      DISPLAY MIRROR holding only safe metadata — the list of run targets
//      (id/name/platform) plus the feedback issue routing target. It is synced
//      over Electric for the web settings UI + pre-clone discovery and is NEVER
//      executed.
//
// `platform` is the discriminator on a run target; targets are arbitrary in
// count/name and multiple per platform are allowed (e.g. `ios-staging`/`ios-prod`).

// Fields shared by every run target regardless of platform.
export interface PlatformCommon {
  enabled?: boolean
  // Working directory for this target, relative to the repo root (e.g.
  // `apps/web`). Rejected server-side if it contains `..`.
  rootDir?: string
  // One-time setup command (e.g. `bun install`), run before build/run.
  setup?: string
  // Extra environment for the build/run commands. PATH/LD_PRELOAD/DYLD_* are
  // stripped server-side.
  env?: Record<string, string>
}

// Identity carried by every run target: a stable `id` (key for last-selected
// memory + the trust hash), a display `name`, and the `platform` discriminator.
interface RunTargetBase {
  id: string
  name: string
}

// web → local dev server embedded in a webview at `url + readyPath`.
export type WebTarget = RunTargetBase &
  PlatformCommon & {
    platform: `web`
    run: string
    url?: string
    port?: number
    readyPath?: string
    injectWidget?: boolean
  }

// android → local SDK emulator (no Docker); build an APK, install + launch it.
export type AndroidTarget = RunTargetBase &
  PlatformCommon & {
    platform: `android`
    build: string
    apk?: string
    installCommand?: string
    avd: string
    systemImage?: string
    applicationId: string
    activity?: string
  }

// ios → local Simulator (macOS only); Linux renders a "needs a Mac" state.
export type IosTarget = RunTargetBase &
  PlatformCommon & {
    platform: `ios`
    scheme: string
    buildCommand?: string
    workspace?: string
    simulator: string
    iosVersion?: string
    bundleId: string
  }

// A single named run target, discriminated on `platform`.
export type RunTarget = WebTarget | AndroidTarget | IosTarget

// The committed `.exponential/config.json` shape — the canonical command source.
export interface ProjectPreviewConfig {
  version: 1
  targets: RunTarget[]
}

// The `projects.preview_config` DB mirror — display-only, never executed.
export interface ProjectPreviewMirror {
  targets: { id: string; name: string; platform: Platform }[]
  feedbackProjectId?: string
}

// --- Zod schemas ---

const platformCommonShape = {
  enabled: z.boolean().optional(),
  rootDir: z.string().optional(),
  setup: z.string().optional(),
  env: z.record(z.string(), z.string()).optional(),
}

export const webTargetSchema = z.object({
  id: z.string(),
  name: z.string(),
  platform: z.literal(`web`),
  run: z.string(),
  url: z.string().optional(),
  port: z.number().int().optional(),
  readyPath: z.string().optional(),
  injectWidget: z.boolean().optional(),
  ...platformCommonShape,
})

export const androidTargetSchema = z.object({
  id: z.string(),
  name: z.string(),
  platform: z.literal(`android`),
  build: z.string(),
  apk: z.string().optional(),
  installCommand: z.string().optional(),
  avd: z.string(),
  systemImage: z.string().optional(),
  applicationId: z.string(),
  activity: z.string().optional(),
  ...platformCommonShape,
})

export const iosTargetSchema = z.object({
  id: z.string(),
  name: z.string(),
  platform: z.literal(`ios`),
  scheme: z.string(),
  buildCommand: z.string().optional(),
  workspace: z.string().optional(),
  simulator: z.string(),
  iosVersion: z.string().optional(),
  bundleId: z.string(),
  ...platformCommonShape,
})

export const runTargetSchema = z.discriminatedUnion(`platform`, [
  webTargetSchema,
  androidTargetSchema,
  iosTargetSchema,
])

export const projectPreviewConfigSchema = z.object({
  version: z.literal(1),
  targets: z.array(runTargetSchema),
})

// The web mutation writes only this DB mirror (display metadata, never
// executed). `targets` is auto-populated by the desktop after it clones +
// parses the repo file.
export const projectPreviewMirrorSchema = z.object({
  targets: z.array(
    z.object({
      id: z.string(),
      name: z.string(),
      platform: platformSchema,
    })
  ),
  feedbackProjectId: z.string().optional(),
})

import { z } from "zod"

// `todo` (the builtin "Todo" status) was retired by EXP-685: migration 0091
// moved every issue to Backlog and only the PG type keeps the orphan label
// (see schema.ts issueStatusEnum). Every input schema rejects the token;
// clients treat it as an unknown wire value.
export const issueStatusValues = [
  `backlog`,
  `in_progress`,
  // PR opened, awaiting review/merge — the coding flow parks issues here
  // between "PR opened" and "PR merged" (which lands them in `done`).
  `in_review`,
  `done`,
  `cancelled`,
  // Terminal resolution: this issue is a duplicate of `issues.duplicateOfId`.
  // Hidden from active lists like done/cancelled.
  `duplicate`,
] as const

// Fixed status categories (EXP-314) — every issue_statuses row belongs to
// one. Hand-mirrors contract.json's issueStatusCategory (parity-locked by
// apps/web's domain-contract test), the same convention as boardIconValues.
export const issueStatusCategoryValues = [
  `backlog`,
  `unstarted`,
  `started`,
  `completed`,
  `cancelled`,
  `duplicate`,
] as const

export type IssueStatusCategory = (typeof issueStatusCategoryValues)[number]

// The categories a CUSTOM status may be created in — everything but
// `duplicate`, which is a fixed single-status category (no + button
// anywhere). Derived, never hand-copied (EXP-707): both the statuses router
// and MCP statuses_create validate against this list.
export const customizableStatusCategoryValues = issueStatusCategoryValues.filter(
  (v): v is Exclude<IssueStatusCategory, `duplicate`> => v !== `duplicate`
) as [
  Exclude<IssueStatusCategory, `duplicate`>,
  ...Exclude<IssueStatusCategory, `duplicate`>[],
]

// The ONE category order every surface speaks (EXP-448): the settings page
// sections, the set-status pickers and the issue-list groups. Lifecycle order
// — it matches the legacy issueStatusOrder for a default team.
export const issueStatusCategoryDisplayOrder: IssueStatusCategory[] = [
  `backlog`,
  `unstarted`,
  `started`,
  `completed`,
  `cancelled`,
  `duplicate`,
]

// Max `started` statuses per team — the pie-clock fill tables are defined
// only up to 4 (2 → [2/4, 3/4], 3 → [1/4..3/4], 4 → [1/5..4/5]).
export const ISSUE_STATUS_STARTED_MAX = 4

// A custom status's dual-written `issues.status` anchor: the builtin enum
// value its category degrades to. Every enum-keyed subsystem (completedAt
// derivations, pr-sync eligibility, MCP tools, old clients) keeps working off
// the anchor while status_id carries the precise row. Builtin rows anchor to
// their own builtin_key (the in_review builtin is why `started` can't simply
// be "the category's only enum value"). `unstarted` has no builtin since
// EXP-685 — its customs anchor to `backlog` ("not started" for every
// enum-only reader). Mirrored by iOS/desktop `anchor()`.
export const CATEGORY_ANCHOR: Record<IssueStatusCategory, IssueStatus> = {
  backlog: `backlog`,
  unstarted: `backlog`,
  started: `in_progress`,
  completed: `done`,
  cancelled: `cancelled`,
  duplicate: `duplicate`,
}

// The 6 locked builtin statuses every team is seeded with — the local
// fallback set clients construct when the issue_statuses shape hasn't synced
// (builtin-actions pattern). Hand-mirrors contract.json issueStatusDefaults
// AND the SQL seed in apps/web/src/db/out/custom/0001_triggers.sql; both
// parity-locked by the web domain-contract test. Colors are seed DATA only —
// builtin rows render via each client's legacy token colors.
export interface BuiltinStatusDefault {
  key: IssueStatus
  category: IssueStatusCategory
  name: string
  color: string
  sortOrder: number
}

export const BUILTIN_STATUS_DEFAULTS: BuiltinStatusDefault[] = [
  {
    key: `backlog`,
    category: `backlog`,
    name: `Backlog`,
    color: `#A1A1AA`,
    sortOrder: 1,
  },
  {
    key: `in_progress`,
    category: `started`,
    name: `In Progress`,
    color: `#EAB308`,
    sortOrder: 1,
  },
  {
    key: `in_review`,
    category: `started`,
    name: `In Review`,
    color: `#22C55E`,
    sortOrder: 2,
  },
  {
    key: `done`,
    category: `completed`,
    name: `Done`,
    color: `#3B82F6`,
    sortOrder: 1,
  },
  {
    key: `cancelled`,
    category: `cancelled`,
    name: `Cancelled`,
    color: `#A1A1AA`,
    sortOrder: 1,
  },
  {
    key: `duplicate`,
    category: `duplicate`,
    name: `Duplicate`,
    color: `#A1A1AA`,
    sortOrder: 1,
  },
]

export const issuePriorityValues = [
  `none`,
  `urgent`,
  `high`,
  `medium`,
  `low`,
] as const

export const teamRoleValues = [`owner`, `member`] as const

// Where an issue came from (issues.source). `user` = filed by a signed-in
// member (the default, and the value the trigger/insert paths leave in place).
// `widget` = filed anonymously through the embeddable feedback widget — those
// rows carry a NULL creator_id (no synthetic user), so clients key the
// "Feedback widget" author label off this value.
// `agent` = filed through the MCP `exponential_report_bug` tool (EXP-496) —
// same NULL creator_id shape as `widget`, but clients label it "Agent".
export const issueSourceValues = [`user`, `widget`, `agent`] as const

// Curated board icon set (boards.icon) — Lucide names. EXP-273: every client
// now renders the SAME Lucide art for these, generated from
// packages/icons/icons.json (`pickable`), so there is no per-platform glyph
// mapping any more. This list must stay byte-equal to that file's `pickable`
// and to contract.json's boardIcon.values (locked by the @exp/icons drift
// test); it is APPEND-ONLY — reordering or removing a name orphans stored
// boards.icon values. NULL icon = clients derive a fallback from repo
// presence. The same set backs the action icon picker.
export const boardIconValues = [
  `code`,
  `square-kanban`,
  `megaphone`,
  `bug`,
  `rocket`,
  `book-open`,
  `globe`,
  `heart`,
  `star`,
  `zap`,
  `wrench`,
  `shield`,
  `package`,
  `terminal`,
  `lightbulb`,
  `message-circle`,
  `palette`,
  `pen-tool`,
  `database`,
  `server`,
  `cloud`,
  `cpu`,
  `layers`,
  `boxes`,
  `folder`,
  `file-text`,
  `calendar`,
  `clock`,
  `users`,
  `user`,
  `flag`,
  `target`,
  `trophy`,
  `lock`,
  `key`,
  `mail`,
  `phone`,
  `bell`,
  `git-branch`,
  `bot`,
  `sparkles`,
  `flask-conical`,
  `shopping-cart`,
  `credit-card`,
  `map-pin`,
  `compass`,
  `briefcase`,
  `graduation-cap`,
  `puzzle`,
  `gamepad-2`,
  `coffee`,
  `plane`,
  `house`,
  `building`,
  `leaf`,
  `sun`,
  `activity`,
  `chart-line`,
  `scale`,
  `car`,
] as const

// How long a soft-deleted (trashed) board is retained before the purge sweep
// hard-deletes it (with all its issues) and reclaims its attachment storage.
// The single source every client mirrors for the restore-window countdown; the
// purge time is computed as deletedAt + this, never stored.
export const BOARD_TRASH_RETENTION_HOURS = 48
export const BOARD_TRASH_RETENTION_MS =
  BOARD_TRASH_RETENTION_HOURS * 60 * 60 * 1000

// How long a `running` coding_sessions row may go without a liveness signal
// (updated_at — the desktop heartbeats it while the claude child is alive)
// before the server-side staleness sweep DELETES it. The desktop's exit hook
// is the normal end path, but it is in-process only — a SIGKILL/panic/power
// loss never fires it, and nothing reconciles on relaunch. The sweep deletes
// rather than flipping to `ended` because the desktop's own-row kill-switch
// treats that flip as a remote kill of the live claude child, while a
// vanished row deliberately never fires it — so even a live session whose
// heartbeats all fail (or a pre-heartbeat desktop build) only loses its
// badge, never its process. Four missed 30-minute heartbeats — tight enough
// that a crashed IDE's phantom badge clears within ~2.5h (EXP-105; the
// desktop also ends its rows on app quit and window close, so the sweep is
// the crash/SIGKILL backstop only), loose enough that flaky pings can't
// strand a live session's badge.
export const CODING_SESSION_STALE_HOURS = 2
export const CODING_SESSION_STALE_MS =
  CODING_SESSION_STALE_HOURS * 60 * 60 * 1000

// Pure staleness predicate shared by the server sweep AND the client render
// guard (EXP-153): a running session is stale once its last liveness signal
// (updated_at — advanced by every desktop heartbeat, equal to the insert time
// when no heartbeat ever landed) plus the staleness window has passed. Clients
// render a stale `running` row as ABSENT — mirroring the sweep's DELETE, never
// as `ended` (that flip is the desktop kill-switch signal) — so a phantom
// badge clears even when the sweep isn't running. Same threshold everywhere,
// no client-side slack: updated_at is server-clock and a live session's is
// ≤30min old, so only >90min of client clock skew could falsely hide one.
export function isCodingSessionStale(
  lastSeenAt: Date,
  now: Date = new Date()
): boolean {
  return lastSeenAt.getTime() + CODING_SESSION_STALE_MS <= now.getTime()
}

// Only `regular` (human) comments exist.
export const commentKindValues = [`regular`] as const

// EXP-741: who posted the comment — a person in a client, or an agent over
// MCP (`comments.create` stamps it from the MCP context; the card header
// carries a "via MCP" caption). Threading is `comments.parent_id`: ONE level
// deep, a reply to a reply flattens to the root.
export const commentSourceValues = [`user`, `mcp`] as const

// Helpdesk conversation vocabulary (SERVER-ONLY — support tables never sync,
// so these stay out of the domain contract). Direction is who wrote the
// message; visibility gates what the anonymous magic-link page may see
// (`internal` notes never leave the member inbox).
export const supportMessageDirectionValues = [`inbound`, `outbound`] as const
export const supportMessageVisibilityValues = [`public`, `internal`] as const

// Notification kinds. Mirrors the `notification_type` pg enum in schema.ts;
// promoted into the contract so the native inbox can label rows.
export const notificationTypeValues = [
  `issue_assigned`,
  `issue_comment`,
  `issue_status_changed`,
  `issue_mention`,
  // New-issue broadcast to team members — currently fired only for
  // feedback-widget submissions (external reporters have no other signal path).
  `issue_created`,
  // PR lifecycle notifications — fan out to assignee + subscribers so the
  // away/phone flow gets "PR opened" and "it's merged" on every channel.
  `pr_opened`,
  `pr_merged`,
  // Helpdesk: an external reporter replied on a support thread (broadcast to
  // team members, mirroring the issue_created feedback broadcast).
  `support_reply`,
] as const

// Pull-request state surfaced on issues.pr_state. Mirrors the GitHub PR state
// machine (written by the MCP open_pr tool + the merge webhook/cron).
export const prStateValues = [`open`, `closed`, `merged`, `draft`] as const

// Lifecycle of a live desktop coding session (coding_sessions.status). A row
// is one interactive terminal session (one embedded terminal + one agent CLI
// child — claude/codex/pi — in one worktree); `running` drives the "coding
// now" badge + Watch/Steer button.
// `in_review` = the agent's PR is open and the terminal is still alive
// awaiting review (EXP-194). The server writes running→in_review on PR open
// and →ended on PR merge (EXP-498: merge ALWAYS ends the session, on every
// merge path); →ended also comes from the explicit kills (steer.killSession,
// codingSessions.end). The →ended flip is the desktop's remote-kill switch.
// The legacy `merged` park state (EXP-358, reversed by EXP-498) was retired
// by EXP-540: migration 0085 moved the last rows to `ended`, and only the PG
// type keeps the orphan label (see schema.ts). Clients infer "done" from
// `in_review` + prState `merged` for old servers.
export const codingSessionStatusValues = [
  `running`,
  `in_review`,
  `ended`,
] as const

// Why a coding session was started (coding_sessions.started_reason,
// documented varchar — NULL = a person started it). Written only by the
// server (`codingSessions.start`), set by the desktop/CLI automation hosts
// when an action's trigger fires (EXP-530); powers the "Automated" badge and
// the Automations tab run history on every client.
// `agent` (EXP-679) = started by another coding session through
// `exponential_sessions_start` — unattended like an automation, so its
// close-out (`exponential_sessions_end`) ENDS it; unlike schedule/event it
// rides every subject (issue, batch, action, builtin, resume).
export const startedReasonValues = [`schedule`, `event`, `agent`] as const

// Who ended a coding session (coding_sessions.ended_by, documented varchar —
// EXP-637). `agent` = the run closed itself via `exponential_sessions_end`
// (the only path that also writes `summary`); `user` =
// steer.killSession; `client` = codingSessions.end (agent exit, tab close,
// app quit); `merge` = a PR merge path; `system` = the stale sweep or account
// deletion. NULL on rows ended by pre-EXP-637 servers.
export const codingSessionEndedByValues = [
  `agent`,
  `user`,
  `client`,
  `merge`,
  `system`,
] as const

// Cap on the agent-written close-out (coding_sessions.summary). A paragraph,
// not a transcript: the runs lists render it in full once expanded.
export const MAX_CODING_SESSION_SUMMARY = 4_000

// Why a user is subscribed to an issue (issue_subscribers.source, pg enum).
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

// Activity-log event kinds (issue_events.type, pg enum). Drives the
// Linear-style timeline on every client.
export const issueEventTypeValues = [
  `status_changed`,
  `assignee_changed`,
  `label_added`,
  `label_removed`,
  `pr_opened`,
  `pr_merged`,
  `board_moved`,
  // EXP-530 automation substrate. `created` rows are SUPPRESSED in every
  // client timeline (the issue header already shows creation) — they exist so
  // event triggers can watch inserts; payload carries priority/status/source
  // so filters never need a join. `priority_changed` renders normally.
  `created`,
  `priority_changed`,
  // EXP-736 relation edges; one row per SIDE (both issues get a line).
  // Payload: { type, relatedIssueId, relatedIdentifier, direction, source }.
  `relation_added`,
  `relation_removed`,
] as const

// EXP-736 issue relations (issue_relations.type, pg enum). ONE row per pair,
// stored in the CANONICAL direction: `blocks` = issue blocks related,
// `parent` = issue is parent of related, `duplicate` = issue is the duplicate
// and related the canonical (mirrors issues.duplicateOfId, dual-written),
// `related` = symmetric, normalized so issue_id < related_issue_id. Clients
// render the inverse label when they sit on the related side.
export const issueRelationTypeValues = [
  `blocks`,
  `parent`,
  `duplicate`,
  `related`,
] as const

// Who created the row: `user` = an explicit pick (never auto-removed),
// `reference` = derived from a `#IDENT` token in the description or a
// comment (delta-removed once the token is gone from every text).
export const issueRelationSourceValues = [`user`, `reference`] as const

// Per-side display labels; byte-locked against contract.json
// forwardLabels/inverseLabels on every client.
export const ISSUE_RELATION_LABELS: Record<
  (typeof issueRelationTypeValues)[number],
  { forward: string; inverse: string }
> = {
  blocks: { forward: `blocks`, inverse: `blocked by` },
  parent: { forward: `parent of`, inverse: `sub-issue of` },
  duplicate: { forward: `duplicate of`, inverse: `duplicated by` },
  related: { forward: `related to`, inverse: `related to` },
}

export type IssueStatus = (typeof issueStatusValues)[number]
export type IssuePriority = (typeof issuePriorityValues)[number]
export type IssueSource = (typeof issueSourceValues)[number]
export type TeamRole = (typeof teamRoleValues)[number]
export type BoardIcon = (typeof boardIconValues)[number]
export type CommentKind = (typeof commentKindValues)[number]
export type CommentSource = (typeof commentSourceValues)[number]
export type NotificationType = (typeof notificationTypeValues)[number]
export type PrState = (typeof prStateValues)[number]
export type CodingSessionStatus = (typeof codingSessionStatusValues)[number]
export type SubscriberSource = (typeof subscriberSourceValues)[number]
export type IssueEventType = (typeof issueEventTypeValues)[number]
export type StartedReason = (typeof startedReasonValues)[number]
export type CodingSessionEndedBy =
  (typeof codingSessionEndedByValues)[number]
export type SupportMessageDirection =
  (typeof supportMessageDirectionValues)[number]
export type SupportMessageVisibility =
  (typeof supportMessageVisibilityValues)[number]

export const issueStatusSchema = z.enum(issueStatusValues)
export const issueStatusCategorySchema = z.enum(issueStatusCategoryValues)
export const issuePrioritySchema = z.enum(issuePriorityValues)
export const issueSourceSchema = z.enum(issueSourceValues)
export const teamRoleSchema = z.enum(teamRoleValues)
export const boardIconSchema = z.enum(boardIconValues)
// EXP-273: actions.icon draws from the SAME curated registry set as
// boards.icon — one picker component, one contract enum, one generated art set
// on every client. Aliased rather than duplicated so the two can never drift.
export const actionIconSchema = boardIconSchema
export const commentKindSchema = z.enum(commentKindValues)
export const commentSourceSchema = z.enum(commentSourceValues)
export const notificationTypeSchema = z.enum(notificationTypeValues)
export const prStateSchema = z.enum(prStateValues)
export const codingSessionStatusSchema = z.enum(codingSessionStatusValues)
export const subscriberSourceSchema = z.enum(subscriberSourceValues)
export const issueEventTypeSchema = z.enum(issueEventTypeValues)
export const issueRelationTypeSchema = z.enum(issueRelationTypeValues)
export const issueRelationSourceSchema = z.enum(issueRelationSourceValues)
export type IssueRelationType = (typeof issueRelationTypeValues)[number]
export type IssueRelationSource = (typeof issueRelationSourceValues)[number]
export const startedReasonSchema = z.enum(startedReasonValues)
export const codingSessionEndedBySchema = z.enum(codingSessionEndedByValues)
export const codingSessionSummarySchema = z
  .string()
  .min(1)
  .max(MAX_CODING_SESSION_SUMMARY)

// ── Action inputs (EXP-257) ──────────────────────────────────────────────────
// Typed inputs an action may declare: members fill them in the run dialog and
// the resolved values are injected into the prompt at launch. `repo`/`board`
// values are the picked ids (resolved to display names server-side); `pr`
// (EXP-259) values are the representative ISSUE id of an issue-linked open
// pull request (a batch PR's picker rows dedupe by prUrl); `icon` (EXP-273)
// values are a curated icon NAME from the shared registry — the one input kind
// whose value is not an id, so it validates against the contract enum instead
// of a team-scoped lookup.

export const actionInputTypeValues = [
  `text`,
  `repo`,
  `board`,
  `pr`,
  `icon`,
  // EXP-530: multi-line free text; identical validation/limits to `text`,
  // only the widget differs (Textarea vs single-line input).
  `textarea`,
] as const
export type ActionInputType = (typeof actionInputTypeValues)[number]

export const MAX_ACTION_INPUTS = 10
export const MAX_ACTION_INPUT_KEY = 32
export const MAX_ACTION_INPUT_LABEL = 100
export const MAX_ACTION_INPUT_PLACEHOLDER = 200
/** Max chars a filled `text` input value may carry (injected into the prompt). */
export const MAX_ACTION_INPUT_TEXT = 4096

// snake_case identifier — the stable key prompt injection and run values use.
const actionInputKeySchema = z
  .string()
  .regex(/^[a-z][a-z0-9_]{0,31}$/, `keys are snake_case, ≤32 chars`)

export const actionInputDefSchema = z.object({
  key: actionInputKeySchema,
  label: z.string().trim().min(1).max(MAX_ACTION_INPUT_LABEL),
  type: z.enum(actionInputTypeValues),
  required: z.boolean().default(false),
  placeholder: z.string().trim().max(MAX_ACTION_INPUT_PLACEHOLDER).optional(),
})
export type ActionInputDef = z.infer<typeof actionInputDefSchema>

export const actionInputsSchema = z
  .array(actionInputDefSchema)
  .max(MAX_ACTION_INPUTS)
  .superRefine((defs, ctx) => {
    const seen = new Set<string>()
    for (const def of defs) {
      if (seen.has(def.key)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: `Duplicate input key "${def.key}"`,
        })
      }
      seen.add(def.key)
    }
  })

// ── Automation triggers (EXP-530, split out of actions in EXP-583) ──────────
// An automation is its OWN row (`automations` table, synced via its own
// shape): a schedule OR an issue-event watcher targeting ONE action, bound to
// ONE runner device (`device_id` = the machine's steer deviceId, never the row
// uuid) with its own agent/model/effort. Many automations per action.
// Deliberately LOCAL-ONLY: no server scheduler — the bound device (desktop GUI
// or the CLI daemon) watches its own Electric sync and starts the run itself.
// The `trigger` jsonb below carries ONLY the when-part (device/enabled/agent
// are real columns). Writes are STRICT (this schema); readers everywhere stay
// tolerant and treat an unparseable/unknown trigger as "never fires". The
// event vocabulary is APPEND-ONLY.

export const actionTriggerEventValues = [
  `created`,
  `status_changed`,
  `assignee_changed`,
  `label_added`,
  `priority_changed`,
  `pr_opened`,
  `pr_merged`,
] as const
export type ActionTriggerEvent = (typeof actionTriggerEventValues)[number]

export const actionScheduleIntervalValues = [
  `daily`,
  `weekly`,
  `monthly`,
] as const
export type ActionScheduleInterval =
  (typeof actionScheduleIntervalValues)[number]

export const MAX_TRIGGER_FILTER_IDS = 20

export interface AutomationScheduleTrigger {
  kind: `schedule`
  interval: ActionScheduleInterval
  /** 0–1439, DEVICE-LOCAL wall clock (no tz field by design). */
  minuteOfDay: number
  /** ISO weekday 1=Mon…7=Sun — required iff weekly. */
  weekday?: number
  /** 1–28 (no 29–31 ambiguity) — required iff monthly. */
  dayOfMonth?: number
}

export interface AutomationEventTriggerFilters {
  /** Applies to every event kind; absent = every board. */
  boardIds?: string[]
  /** label_added only: the added label. */
  labelIds?: string[]
  /** created + priority_changed only: the (new) priority. */
  priorities?: IssuePriority[]
  /** status_changed only: target issue_statuses rows. */
  toStatusIds?: string[]
}

export interface AutomationEventTrigger {
  kind: `event`
  event: ActionTriggerEvent
  filters?: AutomationEventTriggerFilters
}

export type AutomationTrigger = AutomationScheduleTrigger | AutomationEventTrigger

// Filter arrays reject empty ([] would silently match nothing) and cap at 20.
const triggerIdArraySchema = z
  .array(z.string().uuid())
  .min(1)
  .max(MAX_TRIGGER_FILTER_IDS)

const automationScheduleTriggerSchema = z
  .strictObject({
    kind: z.literal(`schedule`),
    interval: z.enum(actionScheduleIntervalValues),
    minuteOfDay: z.number().int().min(0).max(1439),
    weekday: z.number().int().min(1).max(7).optional(),
    dayOfMonth: z.number().int().min(1).max(28).optional(),
  })
  .superRefine((t, ctx) => {
    const issue = (message: string) =>
      ctx.addIssue({ code: z.ZodIssueCode.custom, message })
    if (t.interval === `weekly` && t.weekday === undefined)
      issue(`weekly requires weekday`)
    if (t.interval === `monthly` && t.dayOfMonth === undefined)
      issue(`monthly requires dayOfMonth`)
    if (t.interval !== `weekly` && t.weekday !== undefined)
      issue(`weekday only applies to weekly`)
    if (t.interval !== `monthly` && t.dayOfMonth !== undefined)
      issue(`dayOfMonth only applies to monthly`)
  })

const automationEventTriggerSchema = z
  .strictObject({
    kind: z.literal(`event`),
    event: z.enum(actionTriggerEventValues),
    filters: z
      .strictObject({
        boardIds: triggerIdArraySchema.optional(),
        labelIds: triggerIdArraySchema.optional(),
        priorities: z
          .array(z.enum(issuePriorityValues))
          .min(1)
          .max(MAX_TRIGGER_FILTER_IDS)
          .optional(),
        toStatusIds: triggerIdArraySchema.optional(),
      })
      .optional(),
  })
  .superRefine((t, ctx) => {
    const issue = (message: string) =>
      ctx.addIssue({ code: z.ZodIssueCode.custom, message })
    if (t.filters?.labelIds && t.event !== `label_added`)
      issue(`labelIds only applies to label_added`)
    if (t.filters?.toStatusIds && t.event !== `status_changed`)
      issue(`toStatusIds only applies to status_changed`)
    if (
      t.filters?.priorities &&
      t.event !== `created` &&
      t.event !== `priority_changed`
    )
      issue(`priorities only applies to created/priority_changed`)
  })

/** Strict WRITE schema (tRPC + MCP); every reader stays tolerant instead. */
export const automationTriggerSchema = z.discriminatedUnion(`kind`, [
  automationScheduleTriggerSchema,
  automationEventTriggerSchema,
])

/** devices.device_id cap — the automation's runner binding. */
export const automationDeviceIdSchema = z.string().min(1).max(128)

export const dateOnlySchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)

// EXP-707: the ONE #rrggbb write schema (labels, statuses, boards, widget
// theme) and the accent every color column defaults to (schema.ts varchar
// defaults hand-mirror it — migrations, not imports).
export const hexColorSchema = z
  .string()
  .regex(/^#[0-9a-fA-F]{6}$/, `Expected #rrggbb`)

export const DEFAULT_ACCENT_COLOR = `#6366f1`

// EXP-707: the ONE uuid shape test (7 copies unified). Zod schemas stay
// per-surface (z.string().uuid() vs the MCP budget refine) but the regex is
// shared.
export const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

// Issue descriptions and comment bodies are plain GFM markdown strings (stored
// in `text` columns). The legacy jsonb `{ text }` envelope was unwrapped; the
// helpers below stay tolerant of any old `{ text }` rows still in flight.

// REV-30: descriptions ride the issues Electric shape to every member's
// clients and feed the idx_issues_fts tsvector expression, which Postgres
// hard-caps at ~1MB — an uncapped multi-MB paste bricks writes and bloats
// every fresh snapshot. 64KB matches MAX_ACTION_BODY, far below either limit.
export const MAX_ISSUE_DESCRIPTION = 64 * 1024

export const issueDescriptionSchema = z.string().max(MAX_ISSUE_DESCRIPTION)

export type IssueDescription = z.infer<typeof issueDescriptionSchema>

// EXP-554: comments may be attachment-only, so the body alone can be empty —
// the "body or attachments" rule lives in the comments router where both are
// visible. Attachments link via attachments.comment_id, never inline markdown.
export const commentBodyWithAttachmentsSchema = z.string().max(10_000)

export const MAX_COMMENT_ATTACHMENTS = 10

export function getCommentBodyText(body: unknown): string {
  if (typeof body === `string`) return body
  if (body && typeof body === `object` && `text` in body) {
    const t = (body as { text?: unknown }).text
    return typeof t === `string` ? t : ``
  }
  return ``
}

export const issueStatusOrder: IssueStatus[] = [
  `backlog`,
  `in_progress`,
  `in_review`,
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

export function formatDateForMutation(date: Date | null | undefined) {
  if (!date) {
    return null
  }

  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, `0`)
  const day = String(date.getDate()).padStart(2, `0`)

  return `${year}-${month}-${day}`
}

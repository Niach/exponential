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
  `music`,
  `camera`,
  `video`,
  `image`,
  `headphones`,
  `mic`,
  `gift`,
  `shopping-bag`,
  `store`,
  `truck`,
  `map`,
  `mountain`,
  `tree-pine`,
  `flame`,
  `droplet`,
  `moon`,
  `anchor`,
  `crown`,
  `gem`,
  `award`,
  `dumbbell`,
  `stethoscope`,
  `microscope`,
  `atom`,
  `brain`,
  `eye`,
  `fingerprint`,
  `hammer`,
  `paintbrush`,
  `calculator`,
  `landmark`,
  `wallet`,
  `tag`,
  `bookmark`,
  `newspaper`,
  `smartphone`,
] as const

// EXP-924: the device icon set (devices.icon), a SECOND curated set next to
// the board one: device types plus OS marks. Byte-equal to icons.json
// `devicePickable` and contract.json's deviceIcon.values, APPEND-ONLY. NULL
// icon = the kind default (`monitor` for a desktop, `server` for a daemon).
export const deviceIconValues = [
  `monitor`,
  `server`,
  `laptop`,
  `os-apple`,
  `os-windows`,
  `os-linux`,
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
  // EXP-801: a message an agent sent a team member over MCP
  // (`exponential_notifications_send`). Issue-less like support_reply; the
  // row carries the sending team's `team_id`. Blocked per recipient by
  // `user_notification_prefs.allow_agent_messages` (own agents always pass).
  `agent_message`,
  // EXP-980: a coding run hit a wall (`coding_sessions.blocked` went null →
  // set: a rate limit today). Sent to the run's OWNER for every run; the row
  // carries `session_id` (+ `team_id`) so the inbox row and the push route to
  // the run. Issue-less even for an issue run: the run is the subject.
  `session_blocked`,
] as const

// Pull-request state surfaced on issues.pr_state. Mirrors the GitHub PR state
// machine (written by the MCP open_pr tool + the merge webhook/cron).
export const prStateValues = [`open`, `closed`, `merged`, `draft`] as const

// Lifecycle of a live desktop coding session (coding_sessions.status). A row
// is one interactive terminal session (one embedded terminal + one agent CLI
// child — claude/codex — in one worktree); `running` drives the "coding
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
// `workflow` (EXP-982) = started by the workflow ENGINE on the runner device
// for one node (an issue or a batch). Unattended like `agent`, but it has no
// parent run: its questions go to a person (`sessions_ask_parent` → user).
export const startedReasonValues = [
  `schedule`,
  `event`,
  `agent`,
  `workflow`,
] as const

// Who ended a coding session (coding_sessions.ended_by, documented varchar —
// EXP-637). `agent` = the run closed itself via `exponential_sessions_end`
// (the only path that also writes `summary`); `user` =
// steer.killSession; `client` = codingSessions.end (agent exit, tab close,
// app quit); `merge` = a PR merge path; `system` = account deletion or a
// withdrawn share; `stale` (EXP-888) = the staleness sweep, the ONE end that
// never kills (devices ignore it, a heartbeat revives it). NULL on rows ended
// by pre-EXP-637 servers.
export const codingSessionEndedByValues = [
  `agent`,
  `user`,
  `client`,
  `merge`,
  `system`,
  `stale`,
] as const

// Cap on the agent-written close-out (coding_sessions.summary). A paragraph,
// not a transcript: the runs lists render it in full once expanded.
export const MAX_CODING_SESSION_SUMMARY = 4_000

// EXP-804: the agent's usage wall as coding_sessions row state. NULL = not
// blocked. A blocked run STAYS `running` — it is still live, steerable and
// killable — so this composes with status the way `needs_input` does rather
// than being a status of its own. Written by the device the moment its agent
// reports the wall (claude's `<synthetic>` credit frame, EXP-784) and cleared
// on the next assistant token. `kind` is the discriminator (contract
// codingSessionBlocked.kinds), `window` names WHICH usage window ran out
// (contract codingSessionBlocked.windows, the same vocabulary
// DeviceUsageWindow.key uses). `resetsAt` is an ISO string or null when the
// agent gave no reset time; `since` is when the device first saw the wall.
export const codingSessionBlockedKindValues = [`rate_limit`] as const
export const codingSessionBlockedWindowValues = [
  `session`,
  `weekly`,
  `model`,
] as const

export interface CodingSessionBlocked {
  kind: string
  agent: string
  window: string
  resetsAt: string | null
  since: string
}

// Every field is `.nullish()` for the same reason deviceAgentUsageSchema's
// are: a newer client sending a field this server does not know, or an
// explicit `null` where this server expects a string, must degrade that field
// rather than 400 the write and leave a rate-limited run reading healthy.
// Vocabulary is NOT enforced here (`kind`/`window` are free strings) so a
// future window name from a newer device still lands; presentation falls back.
export const codingSessionBlockedSchema = z.object({
  kind: z.string().max(64).nullish(),
  agent: z.string().max(64).nullish(),
  window: z.string().max(64).nullish(),
  resetsAt: z.string().max(64).nullish(),
  since: z.string().max(64).nullish(),
})

// EXP-879: the pictures a run published of its own work (coding_sessions
// .results). A flat, ORDERED array; NULL or [] = none. Each entry names a
// `topic` (one screen or flow, e.g. `chatui`) and a `label` (one picture in
// it, e.g. `ios`/`android`/`web`); (topic, label) is the upsert key. The
// bytes live in `session_attachments` (server-only), clients derive
// `/api/attachments/{attachmentId}` — no stored URL. `width`/`height` are the
// server-probed pixel dimensions (null when probing failed) so tiles size
// before the image loads. The caps are LOAD-BEARING: the row re-ships WHOLE
// through the coding-sessions shape on every heartbeat, so a big array
// multiplies sync traffic by every live client. Escape hatch if they ever
// pinch: a `session_results` table + its own shape.
export const SESSION_RESULT_TEXT_MAX = 80
export const SESSION_RESULTS_MAX = 60

export interface CodingSessionResult {
  topic: string
  label: string
  attachmentId: string
  width: number | null
  height: number | null
}

// Tolerant for the same reason codingSessionBlockedSchema is: a malformed
// entry degrades (readers drop it) rather than 400 the whole write.
export const codingSessionResultSchema = z.object({
  topic: z.string().max(SESSION_RESULT_TEXT_MAX).nullish(),
  label: z.string().max(SESSION_RESULT_TEXT_MAX).nullish(),
  attachmentId: z.string().max(64).nullish(),
  width: z.number().int().nullish(),
  height: z.number().int().nullish(),
})

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

// EXP-778: what a personal pin points at (pins.kind, pg enum). One nullable
// target column per kind (issue_id / session_id / action_id) with an FK
// cascade, so a pin dies with its target instead of dangling.
export const pinKindValues = [`issue`, `session`, `action`] as const

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
  // EXP-630 estimates: { from, to } point values (null = unset). Renders as
  // a plain line; never folds (mirrors treat it like any unknown kind).
  `estimate_changed`,
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
export type DeviceIcon = (typeof deviceIconValues)[number]
export type CommentKind = (typeof commentKindValues)[number]
export type CommentSource = (typeof commentSourceValues)[number]
export type NotificationType = (typeof notificationTypeValues)[number]
export type PrState = (typeof prStateValues)[number]
export type CodingSessionStatus = (typeof codingSessionStatusValues)[number]
export type SubscriberSource = (typeof subscriberSourceValues)[number]
export type PinKind = (typeof pinKindValues)[number]
export type IssueEventType = (typeof issueEventTypeValues)[number]
export type StartedReason = (typeof startedReasonValues)[number]
export type CodingSessionEndedBy =
  (typeof codingSessionEndedByValues)[number]
export type CodingSessionBlockedKind =
  (typeof codingSessionBlockedKindValues)[number]
export type CodingSessionBlockedWindow =
  (typeof codingSessionBlockedWindowValues)[number]
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
export const deviceIconSchema = z.enum(deviceIconValues)
export const commentKindSchema = z.enum(commentKindValues)
export const commentSourceSchema = z.enum(commentSourceValues)
export const notificationTypeSchema = z.enum(notificationTypeValues)
export const prStateSchema = z.enum(prStateValues)
export const codingSessionStatusSchema = z.enum(codingSessionStatusValues)
export const subscriberSourceSchema = z.enum(subscriberSourceValues)
export const pinKindSchema = z.enum(pinKindValues)
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

// EXP-825: every input is a PICK. The free-text kinds (`text`, EXP-530's
// `textarea`) are retired — whatever the requester types beside a subject
// rides the start as `prompt` (see MAX_START_PROMPT) and reaches the run as
// an "Additional instructions" section, so an action never declares a field
// for it.
export const actionInputTypeValues = [`repo`, `board`, `pr`, `icon`] as const
export type ActionInputType = (typeof actionInputTypeValues)[number]

// EXP-792: team MCP servers (server-only `mcp_servers` rows, never synced).
// `transport` = how the agent reaches the server; `auth` = how the DEVICE
// authenticates to it: `none`, an OAuth sign-in the device executes, or a
// `secret` typed on the device (a header value for http, an env value for
// stdio). Documented varchars mirrored in contract.json (`mcpTransport`,
// `mcpAuth`); the server never stores a credential either way.
export const mcpTransportValues = [`http`, `stdio`] as const
export type McpTransport = (typeof mcpTransportValues)[number]
export const mcpAuthValues = [`none`, `oauth`, `secret`] as const
export type McpAuth = (typeof mcpAuthValues)[number]
export const mcpTransportSchema = z.enum(mcpTransportValues)
export const mcpAuthSchema = z.enum(mcpAuthValues)
/** Team MCP server names: ≤64 chars, unique per team. */
export const MAX_MCP_SERVER_NAME = 64
/** Header/env NAMES (never values) an MCP server row may declare. */
export const MAX_MCP_SERVER_NAMES = 16
/** A header or env-var NAME — the only shape the server accepts (values live on the device). */
export const mcpVariableNameSchema = z
  .string()
  .min(1)
  .max(64)
  .regex(/^[A-Za-z_][A-Za-z0-9_-]*$/, `must be a header or variable name`)

export const MAX_ACTION_INPUTS = 10
export const MAX_ACTION_INPUT_KEY = 32
export const MAX_ACTION_INPUT_LABEL = 100
export const MAX_ACTION_INPUT_PLACEHOLDER = 200
/** EXP-825: the composer's field hint while the action is picked. */
export const MAX_ACTION_PROMPT_PLACEHOLDER = 200
export const actionPromptPlaceholderSchema = z
  .string()
  .trim()
  .max(MAX_ACTION_PROMPT_PLACEHOLDER)
/** Max chars a filled input VALUE may carry (ids and icon names in practice; the cap is contract-locked). */
export const MAX_ACTION_INPUT_TEXT = 4096

/**
 * EXP-825: the free text a start carries beside its subject — the chat
 * prompt, or additional instructions for an issue/batch/action run — in the
 * steer-image-message shape (prose, then up to MAX_START_PROMPT_IMAGES
 * trailing `![image](/api/attachments/<id>)` embed lines). The cap covers
 * the WHOLE string. Parity-locked with contract.json `startPrompt`.
 */
export const MAX_START_PROMPT = 16384
export const MAX_START_PROMPT_IMAGES = 4
export const startPromptSchema = z
  .string()
  .max(MAX_START_PROMPT)
  .refine((value) => !value.includes(`\u0000`), `contains NUL bytes`)

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

// EXP-630: story points — a non-negative integer (issues.estimate). The
// TEAM picks the scale (`teams.estimation_type`, contract `issueEstimation`,
// default `none` = estimates off): the value stays a point number whatever
// the scale, so switching scales never rewrites issues; t-shirt sizes are
// the fibonacci points worn as XS…XL (Linear's mapping).
export const ISSUE_ESTIMATE_MAX = 1000
export const issueEstimateSchema = z.number().int().min(0).max(ISSUE_ESTIMATE_MAX)

export const issueEstimationValues = [`none`, `exponential`, `fibonacci`, `linear`, `tshirt`] as const
export type IssueEstimation = (typeof issueEstimationValues)[number]
export const issueEstimationSchema = z.enum(issueEstimationValues)

export const ISSUE_ESTIMATION_SCALES: Record<Exclude<IssueEstimation, `none`>, readonly number[]> = {
  exponential: [1, 2, 4, 8, 16],
  fibonacci: [1, 2, 3, 5, 8],
  linear: [1, 2, 3, 4, 5],
  tshirt: [1, 2, 3, 5, 8],
}
export const ISSUE_ESTIMATE_TSHIRT_LABELS = [`XS`, `S`, `M`, `L`, `XL`] as const

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

// ── Workflows (EXP-978/981) ─────────────────────────────────────────────────
// A workflow = a picked set of issues of ONE repository, run as a DAG: the
// `blocks` relations among them are the edges, a parent with its sub-issues is
// ONE compound node (run as a batch), and each node's session + PR is its
// state. The contract keys carry the `wf` prefix: `workflowStatus` was already
// the agent feed's word for a Claude Code workflow TOOL run.
//
// All five are documented varchars (not pg enums): the node lifecycle grows
// with the engine's phases, and an `ALTER TYPE` per phase buys nothing a zod
// schema at the writers does not.
export const wfStatusValues = [
  `draft`,
  `running`,
  `paused`,
  `done`,
  `cancelled`,
] as const
export const wfNodeStateValues = [
  // Filed mid-run by a session; not part of the graph until admitted.
  `proposed`,
  // Waiting on a blocker.
  `blocked`,
  // Every blocker satisfied `start_on`; the scheduler may start it.
  `ready`,
  `running`,
  // Needs a person: a question, a rate limit, a login. The ONLY amber state
  // and the only one that pushes.
  `waiting`,
  `in_review`,
  // Merging a moved upstream in.
  `updating`,
  `landed`,
  `failed`,
  `skipped`,
] as const
export const wfNodeKindValues = [`contract`, `leaf`, `integration`] as const
export const wfStartOnValues = [`contract`, `pr_open`, `landed`] as const
export const wfRiskValues = [`low`, `medium`, `high`] as const

export type WfStatus = (typeof wfStatusValues)[number]
export type WfNodeState = (typeof wfNodeStateValues)[number]
export type WfNodeKind = (typeof wfNodeKindValues)[number]
export type WfStartOn = (typeof wfStartOnValues)[number]
export type WfRisk = (typeof wfRiskValues)[number]

export const wfStatusSchema = z.enum(wfStatusValues)
export const wfNodeStateSchema = z.enum(wfNodeStateValues)
export const wfNodeKindSchema = z.enum(wfNodeKindValues)
export const wfStartOnSchema = z.enum(wfStartOnValues)
export const wfRiskSchema = z.enum(wfRiskValues)

/** `contract.workflow`, hand-mirrored (drift-tested). */
export const WORKFLOW_MAX_PARALLEL_DEFAULT = 3
export const WORKFLOW_MAX_ISSUES = 50
export const WORKFLOW_MAX_PARALLEL_CAP = 8
export const WORKFLOW_DECISIONS_MAX = 65536

/** EXP-1029: the agents a workflow may run on (contract `workflowLaunch`). */
export const workflowLaunchAgentValues = [`claude`, `codex`] as const
export type WorkflowLaunchAgent = (typeof workflowLaunchAgentValues)[number]

/**
 * EXP-1029: THE workflow launch — what every node run and every agent review
 * reads, once `normalizeWorkflowLaunch` (web `lib/workflow-launch.ts`,
 * desktop `coding::workflows::launch`) has folded the stored jsonb into it.
 *
 * Two models, no more: `model` is the CHEAP one (leaf nodes, and the `Task`
 * subagents inside every node run), `strongModel` the capable one (contract
 * nodes, integration nodes, `risk: high` nodes and EVERY agent review). The
 * gate choice is gone — the agent reviews every node and the one human
 * review is the final PR — and `startOn` is fixed to `contract`. A new
 * workflow takes both models from the creating device's agent defaults
 * (`DeviceWorkflowDefaults`); the workflow screen shows no settings panel.
 */
export interface WorkflowLaunch {
  agent: WorkflowLaunchAgent
  /** An agent profile id on the runner device; absent = its active login. */
  account?: string
  model: string
  strongModel: string
}

/** Contract `workflowLaunch` per-agent defaults, hand-mirrored (drift-tested):
 *  what `normalizeWorkflowLaunch` fills in when the stored row names none. */
export const WORKFLOW_LAUNCH_DEFAULTS: Record<
  WorkflowLaunchAgent,
  Pick<WorkflowLaunch, `model` | `strongModel`>
> = {
  claude: { model: `opus`, strongModel: `fable` },
  codex: { model: `gpt-5.6-sol`, strongModel: `gpt-5.6-luna` },
}

/**
 * `workflows.launch` AS STORED (jsonb, so no migration): every field is
 * optional; an absent one falls back to the runner device's defaults.
 *
 * EXP-1029 adds `strongModel` and DEPRECATES the per-phase pins,
 * `subagentModel`, `reviewModel`, `effort` and `maxParallel`: none of them is
 * part of [`WorkflowLaunch`] any more. `normalizeWorkflowLaunch` folds a set
 * `contractModel` / `integrationModel` / `riskModel` / `reviewModel` into
 * `strongModel` and drops the rest. EXP-1032 removed them from every writer
 * and every settings UI (`workflows.update` stores the normalized four keys
 * and nothing else); older clients still SEND them, so the schema keeps
 * accepting them.
 */
export interface WorkflowLaunchStored {
  agent?: string | null
  /** The cheap model (`WorkflowLaunch.model`). */
  model?: string | null
  /** EXP-1029: the strong model. Absent on rows written before it. */
  strongModel?: string | null
  /** @deprecated EXP-1029 — folds into `strongModel`; EXP-1014 removes it. */
  contractModel?: string | null
  /** @deprecated EXP-1029 — folds into `strongModel`; EXP-1014 removes it. */
  integrationModel?: string | null
  /** @deprecated EXP-1029 — folds into `strongModel`; EXP-1014 removes it. */
  riskModel?: string | null
  /** @deprecated EXP-1029 — subagents run on `model`; EXP-1014 removes it. */
  subagentModel?: string | null
  /** @deprecated EXP-1029 — not part of `WorkflowLaunch`; EXP-1014 removes it. */
  effort?: string | null
  /** An agent profile id on the runner device. */
  account?: string | null
  /** @deprecated EXP-1029 — not part of `WorkflowLaunch` (the engine caps at
   *  contract `workflow.maxParallelDefault`); EXP-1014 removes it. */
  maxParallel?: number | null
  /** @deprecated EXP-1029 — every review runs on `strongModel`; EXP-1014
   *  removes it. */
  reviewModel?: string | null
}

/**
 * EXP-1002: what a NEW workflow starts configured as, PER AGENT. Explicit on
 * every field a run reads, so a person opening the panel sees the launch
 * rather than five rows reading "Default".
 *
 * @deprecated EXP-1029 — a new workflow takes `model` / `strongModel` from the
 * creating device's agent defaults (`DeviceWorkflowDefaults`); EXP-1014
 * replaces this table with `WORKFLOW_LAUNCH_DEFAULTS`.
 */
export const WORKFLOW_DEFAULT_LAUNCH_BY_AGENT: Record<string, WorkflowLaunchStored> = {
  claude: {
    agent: `claude`,
    model: `opus`,
    contractModel: `fable`,
    integrationModel: `fable`,
    riskModel: `fable`,
    subagentModel: `opus`,
  },
  // Codex has no subagent model to pin (claude-only), so its entry is the
  // four that a node run actually reads.
  codex: {
    agent: `codex`,
    model: `gpt-5.6-sol`,
    contractModel: `gpt-5.6-luna`,
    integrationModel: `gpt-5.6-luna`,
    riskModel: `gpt-5.6-luna`,
  },
}

/** The launch a new workflow is created with: the default agent's entry.
 *  @deprecated EXP-1029 — see `WORKFLOW_DEFAULT_LAUNCH_BY_AGENT`. */
export const WORKFLOW_DEFAULT_LAUNCH: WorkflowLaunchStored =
  WORKFLOW_DEFAULT_LAUNCH_BY_AGENT.claude!

/** The stored shape's wire schema (`workflows.update`). Keys stay `.strict()`
 *  so a typo is refused; the deprecated keys stay accepted for old clients
 *  (EXP-1032 normalizes them away before anything is stored). */
export const workflowLaunchSchema = z
  .object({
    agent: z.string().max(16).nullish(),
    model: z.string().max(64).nullish(),
    strongModel: z.string().max(64).nullish(),
    contractModel: z.string().max(64).nullish(),
    integrationModel: z.string().max(64).nullish(),
    riskModel: z.string().max(64).nullish(),
    subagentModel: z.string().max(64).nullish(),
    effort: z.string().max(32).nullish(),
    account: z.string().max(64).nullish(),
    maxParallel: z.number().int().min(1).max(WORKFLOW_MAX_PARALLEL_CAP).nullish(),
    reviewModel: z.string().max(64).nullish(),
  })
  .strict()

/** EXP-1029: a device's WORKFLOW model defaults — `launch_defaults.workflow`
 *  on the synced devices row, `workflowModel` / `workflowStrongModel` in the
 *  desktop's settings.json. What a new workflow's launch is seeded from
 *  (EXP-1014 reads it at creation; EXP-1020 owns the "Workflow settings"
 *  sub-shell that edits it). Model names belong to the default account's
 *  agent vocabulary. */
export interface DeviceWorkflowDefaults {
  model: string
  strongModel: string
}

/** EXP-1029: a device's agent defaults as ONE flattened view — the shape the
 *  settings UI edits (EXP-1020) and the workflow creator reads (EXP-1014).
 *  `account` is a profile id (null = the default agent's active login) and
 *  IMPLIES the agent; `model` / `subagentModel` are that agent's launch
 *  defaults; `workflow` seeds new workflows. Stored across
 *  `DeviceLaunchDefaults.defaultAccount`, `.agents[agent]` and `.workflow`. */
export interface DeviceAgentDefaults {
  account: string | null
  model: string
  subagentModel: string
  workflow: DeviceWorkflowDefaults
}

/** Contract `deviceAgentDefaults`, hand-mirrored (drift-tested): what a
 *  device that never set anything is read as — the desktop's own fresh-
 *  install defaults (`coding::settings`, wired to the SAME contract
 *  constants): `fable`, a BLANK subagent model (= the CLI's own default),
 *  and the workflow pair. The issue proposed `opus` / `opus` for the first
 *  two; that would change what every fresh device runs, so the contract
 *  names reality and the human review may overrule it here. */
export const DEVICE_AGENT_DEFAULTS: DeviceAgentDefaults = {
  account: null,
  model: `fable`,
  subagentModel: ``,
  workflow: { model: `opus`, strongModel: `fable` },
}

/** `workflows.metrics`: the plan's shape (written by the server layout) plus,
 *  from the engine's phases on, the run's counters. */
export interface WorkflowMetricsJson {
  nodes: number
  edges: number
  depth: number
  width: number
  /** One entry per blocking cycle: its issue identifiers. Empty = startable. */
  cycles: string[][]
  /** `<fromNodeId>\n<toNodeId>` of every edge inside a cycle. */
  cycleEdges?: string[]
  [counter: string]: unknown
}

/** A `touches` glob: what a node expects to change (pre-serialises obvious
 *  collisions). */
export const workflowTouchesSchema = z.array(z.string().min(1).max(256)).max(64)

// ── Agent review (EXP-984) ──────────────────────────────────────────────────
export const wfReviewVerdictValues = [`approve`, `request_changes`] as const
export type WfReviewVerdict = (typeof wfReviewVerdictValues)[number]
export const wfReviewVerdictSchema = z.enum(wfReviewVerdictValues)

/** Review rounds before a node stops bouncing and waits for a person. */
export const WORKFLOW_MAX_REVIEW_ROUNDS = 3

/** An executable check the reviewer RAN (contract tests on the trunk). An
 *  agent's opinion is advisory; a passing oracle is evidence. */
export interface WorkflowReviewOracle {
  command: string
  passed: boolean
}

/** `workflow_nodes.review`: the latest submitted verdict. */
export interface WorkflowNodeReview {
  verdict: WfReviewVerdict
  findings: string
  oracle: WorkflowReviewOracle | null
  /** The model that reviewed (a `risk: high` node: never its author's). */
  model: string | null
  round: number
  at: string
  /** The commit the reviewer reviewed (the PR head's sha, 7-40 lowercase
   *  hex). The engine lands a node ONLY while its PR head still matches:
   *  a push after the review means a fresh review. Absent on old rows. */
  head?: string
}

// FEED-51: the review prompt asks for the EXACT commands run, and a
// multi-platform oracle (web + desktop + a native suite) did not fit 500.
export const WORKFLOW_REVIEW_ORACLE_COMMAND_MAX = 2000

export const workflowReviewOracleSchema = z
  .object({
    command: z.string().min(1).max(WORKFLOW_REVIEW_ORACLE_COMMAND_MAX),
    passed: z.boolean(),
  })
  .strict()

/** `WorkflowNodeReview.head`: a git commit sha, abbreviated or full. */
export const workflowReviewHeadSchema = z.string().regex(/^[0-9a-f]{7,40}$/)

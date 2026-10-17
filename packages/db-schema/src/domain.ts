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
  `brush-cleaning`,
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

// EXP-741: who posted the comment — a person in a client, an agent over
// MCP (`comments.create` stamps it from the MCP context; the card header
// carries a "via MCP" caption), or (SLOP-4) the widget REPORTER through
// their magic-link page (`author_id` NULL; the card names the submission's
// reporter and carries a "reporter" caption). Threading is
// `comments.parent_id`: ONE level deep, a reply to a reply flattens to the
// root.
export const commentSourceValues = [`user`, `mcp`, `reporter`] as const

// SLOP-4: who may read a comment. `team` never leaves the team; `reporter`
// shows on the widget reporter's magic-link page and, written by a member,
// is emailed to the submission's reporter address. A reporter-written
// comment is always `reporter`.
export const commentAudienceValues = [`team`, `reporter`] as const

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
  // SLOP-4: the widget reporter answered on their issue (issue-scoped, to
  // subscribers + assignee like a comment; replaced the issue-less
  // `support_reply` of the standalone helpdesk).
  `reporter_reply`,
  // EXP-801: a message an agent sent a team member over MCP
  // (`exponential_notifications_send`). Issue-less; the row carries the
  // sending team's `team_id`. Blocked per recipient by
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
//
// EXP-933: an entry may instead carry `text` (GFM, the run's REPORT for that
// topic: what it did, headings/bullets/code/issue refs) with `label` and
// `attachmentId` null — one text entry per topic, rendered ABOVE the topic's
// pictures, so topics in first-seen order read as a report. Old readers drop
// label-less entries (every parser requires label + attachmentId), so the
// shape change is backward safe. Text caps are load-bearing for the same
// heartbeat reason as the count cap.
export const SESSION_RESULT_TEXT_MAX = 80
export const SESSION_RESULTS_MAX = 60
// EXP-1154: the report IS the PR body, so a topic's text is a SECTION (2-3
// sentences), not a document: 600 per topic, 4000 per run. A text entry may
// also carry `files` (repo-relative paths the topic touched; the Guide lists
// them with their diff counts and the PR body bullets them).
export const SESSION_RESULT_REPORT_MAX = 600
export const SESSION_RESULTS_REPORT_TOTAL_MAX = 4000
export const SESSION_RESULT_FILES_MAX = 40
export const SESSION_RESULT_FILE_PATH_MAX = 300
export const SESSION_RESULTS_FILES_TOTAL_MAX = 120
// EXP-1172: a picture may carry `inline: true` (filed by
// `exponential_sessions_show` while the run worked: it renders in the run's
// transcript at the call and folds under "Earlier" on the Results face) and a
// short `caption` (the call's `text`). Old readers ignore both fields.
export const SESSION_RESULT_CAPTION_MAX = 200
export const SESSION_SHOW_DEFAULT_TOPIC = `Progress`
// EXP-1251: a text entry's `prUrl` (the PR its topic belongs to) and every
// entry's server write stamp `at` (epoch ms, EXP-1245: the thread's turns).
export const SESSION_RESULT_PR_URL_MAX = 300

export interface GuideEntry {
  topic: string
  /** null on a text entry. */
  label: string | null
  /** null on a text entry. */
  attachmentId: string | null
  width: number | null
  height: number | null
  /** EXP-933: the topic's GFM report text; absent/null on a picture. */
  text?: string | null
  /** EXP-1154: the paths a TEXT entry's topic touched; absent without any. */
  files?: string[] | null
  /** EXP-1172: a `sessions_show` picture; absent on every other entry. */
  inline?: boolean
  /** EXP-1172: the show call's caption; absent without one. */
  caption?: string | null
  /** EXP-1245: the server's write stamp (epoch ms), set on every write so a
   *  client slots the entry into the turn it was filed in; absent on entries
   *  filed before it. */
  at?: number | null
  /** EXP-1251: on a TEXT entry, the PR its topic belongs to (a run that
   *  stacks a second PR scopes its topics); absent = every PR of the run. */
  prUrl?: string | null
  /** EXP-1284: the session that wrote the entry (stamped server-side on
   *  every write): a thread's ONE Guide holds every session's entries, so
   *  a session block shows its own (`sessionGuideEntries`), a moved session
   *  takes its own along, and a later session never overwrites another's
   *  topic. Absent on entries filed before threads. */
  sessionId?: string | null
}

// Tolerant for the same reason codingSessionBlockedSchema is: a malformed
// entry degrades (readers drop it) rather than 400 the whole write.
export const guideEntrySchema = z.object({
  topic: z.string().max(SESSION_RESULT_TEXT_MAX).nullish(),
  label: z.string().max(SESSION_RESULT_TEXT_MAX).nullish(),
  attachmentId: z.string().max(64).nullish(),
  width: z.number().int().nullish(),
  height: z.number().int().nullish(),
  text: z.string().max(SESSION_RESULT_REPORT_MAX).nullish(),
  files: z
    .array(z.string().max(SESSION_RESULT_FILE_PATH_MAX))
    .max(SESSION_RESULT_FILES_MAX)
    .nullish(),
  inline: z.boolean().nullish(),
  caption: z.string().max(SESSION_RESULT_CAPTION_MAX).nullish(),
  at: z.number().int().nonnegative().nullish(),
  prUrl: z.string().max(SESSION_RESULT_PR_URL_MAX).nullish(),
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
  // EXP-630 estimates: { from, to } point values (null = unset). Folds like
  // priority (lib/activity/fold.ts).
  `estimate_changed`,
  // FEED-87 renames: { from, to } title strings. Folds like priority; no
  // notification.
  `title_changed`,
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
export type CommentAudience = (typeof commentAudienceValues)[number]
export type NotificationType = (typeof notificationTypeValues)[number]
export type PrState = (typeof prStateValues)[number]
export type SubscriberSource = (typeof subscriberSourceValues)[number]
export type PinKind = (typeof pinKindValues)[number]
export type IssueEventType = (typeof issueEventTypeValues)[number]
export type CodingSessionBlockedKind =
  (typeof codingSessionBlockedKindValues)[number]
export type CodingSessionBlockedWindow =
  (typeof codingSessionBlockedWindowValues)[number]

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
export const commentAudienceSchema = z.enum(commentAudienceValues)
export const notificationTypeSchema = z.enum(notificationTypeValues)
export const prStateSchema = z.enum(prStateValues)
export const subscriberSourceSchema = z.enum(subscriberSourceValues)
export const pinKindSchema = z.enum(pinKindValues)
export const issueEventTypeSchema = z.enum(issueEventTypeValues)
export const issueRelationTypeSchema = z.enum(issueRelationTypeValues)
export const issueRelationSourceSchema = z.enum(issueRelationSourceValues)
export type IssueRelationType = (typeof issueRelationTypeValues)[number]
export type IssueRelationSource = (typeof issueRelationSourceValues)[number]

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
export const actionInputTypeValues = [`repo`, `board`, `pr`, `issue`] as const
export type ActionInputType = (typeof actionInputTypeValues)[number]

// EXP-792: team MCP servers (server-only `mcp_servers` rows, never synced).
// `transport` = how the agent reaches the server; `auth` = how a MEMBER
// authenticates to it: `none`, an OAuth sign-in the server runs (hosted
// callback), or a typed `secret` (a header value for http, an env value for
// stdio). Either credential is held by the server per member, encrypted
// (`mcp_credentials`). Documented varchars mirrored in contract.json
// (`mcpTransport`, `mcpAuth`).
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
/** A header or env-var NAME — the only shape a server ROW accepts (values live in `mcp_credentials`). */
export const mcpVariableNameSchema = z
  .string()
  .min(1)
  .max(64)
  .regex(/^[A-Za-z_][A-Za-z0-9_-]*$/, `must be a header or variable name`)

/** Env names a stdio MCP server row may NOT declare: the launcher's own
 * process environment (a member's secret must never become `PATH`,
 * `NODE_OPTIONS`, the agent's config dir or base URL, Exponential's own
 * token vars, git's). Exact names + prefixes, case-insensitive. */
export const RESERVED_MCP_ENV_NAMES = [`PATH`, `HOME`, `SHELL`, `USER`, `TMPDIR`, `PWD`] as const
export const RESERVED_MCP_ENV_PREFIXES = [
  `LD_`,
  `DYLD_`,
  `NODE_`,
  `ANTHROPIC_`,
  `CLAUDE_`,
  `CODEX_`,
  `OPENAI_`,
  `EXP_`,
  `GIT_`,
  `BUN_`,
] as const
export function isReservedMcpEnvName(name: string): boolean {
  const upper = name.toUpperCase()
  return (
    (RESERVED_MCP_ENV_NAMES as readonly string[]).includes(upper) ||
    RESERVED_MCP_ENV_PREFIXES.some((prefix) => upper.startsWith(prefix))
  )
}
/** A stdio server's env-var NAME: a variable name that is not reserved. */
export const mcpEnvNameSchema = mcpVariableNameSchema.refine(
  (name) => !isReservedMcpEnvName(name),
  { error: (issue) => `${String(issue.input)} is reserved and cannot be set by an MCP server` }
)

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

// ── Agent triggers (EXP-530 → SLOP-2 → EXP-1291: on team agents) ──────────
// A team agent (`sessions.kind = agent`) carries its triggers (`sessions
// .triggers` jsonb). A trigger is only its WHEN (a schedule or an event
// watcher) plus `id` + `enabled`: the agent itself is the runner (its device,
// account, model). LOCAL-ONLY: the agent's device watches its own Electric
// sync and posts the firing into the agent's channel. An event names its
// `source` (`exponential` today). Writes are STRICT (agentTriggerSchema);
// readers stay tolerant (an unknown trigger never fires). The event
// vocabulary is APPEND-ONLY.

export const agentTriggerEventValues = [
  `created`,
  `status_changed`,
  `assignee_changed`,
  `label_added`,
  `priority_changed`,
  `pr_opened`,
  `pr_merged`,
] as const
export type AgentTriggerEvent = (typeof agentTriggerEventValues)[number]

export const agentScheduleIntervalValues = [
  `daily`,
  `weekly`,
  `monthly`,
] as const
export type AgentScheduleInterval =
  (typeof agentScheduleIntervalValues)[number]

export const agentTriggerSourceValues = [`exponential`] as const
export type AgentTriggerSource = (typeof agentTriggerSourceValues)[number]

export const MAX_TRIGGER_FILTER_IDS = 20

export interface ScheduleTriggerWhen {
  kind: `schedule`
  interval: AgentScheduleInterval
  /** 0–1439, DEVICE-LOCAL wall clock (no tz field by design). */
  minuteOfDay: number
  /** ISO weekday 1=Mon…7=Sun — required iff weekly. */
  weekday?: number
  /** 1–28 (no 29–31 ambiguity) — required iff monthly. */
  dayOfMonth?: number
}

export interface EventTriggerFilters {
  /** Applies to every event kind; absent = every board. */
  boardIds?: string[]
  /** label_added only: the added label. */
  labelIds?: string[]
  /** created + priority_changed only: the (new) priority. */
  priorities?: IssuePriority[]
  /** status_changed only: target issue_statuses rows. */
  toStatusIds?: string[]
}

export interface EventTriggerWhen {
  kind: `event`
  event: AgentTriggerEvent
  filters?: EventTriggerFilters
}

export type TriggerWhen = ScheduleTriggerWhen | EventTriggerWhen


// Filter arrays reject empty ([] would silently match nothing) and cap at 20.
const triggerIdArraySchema = z
  .array(z.string().uuid())
  .min(1)
  .max(MAX_TRIGGER_FILTER_IDS)


const scheduleTriggerShape = {
  kind: z.literal(`schedule`),
  interval: z.enum(agentScheduleIntervalValues),
  minuteOfDay: z.number().int().min(0).max(1439),
  weekday: z.number().int().min(1).max(7).optional(),
  dayOfMonth: z.number().int().min(1).max(28).optional(),
}

const eventFilterShape = {
  boardIds: triggerIdArraySchema.optional(),
  labelIds: triggerIdArraySchema.optional(),
  priorities: z
    .array(z.enum(issuePriorityValues))
    .min(1)
    .max(MAX_TRIGGER_FILTER_IDS)
    .optional(),
  toStatusIds: triggerIdArraySchema.optional(),
}

const eventTriggerShape = {
  kind: z.literal(`event`),
  // Optional on the wire (an agent may omit it), always stored.
  source: z.enum(agentTriggerSourceValues).default(`exponential`),
  event: z.enum(agentTriggerEventValues),
  filters: z.strictObject(eventFilterShape).optional(),
}

function refineSchedule(
  t: { interval: AgentScheduleInterval; weekday?: number; dayOfMonth?: number },
  ctx: z.RefinementCtx
): void {
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
}

function refineEvent(
  t: { event: AgentTriggerEvent; filters?: EventTriggerFilters },
  ctx: z.RefinementCtx
): void {
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
}



/** The when-part of a stored trigger (schedule or event, no runner fields). */
export function triggerWhenPart(trigger: AgentTrigger): TriggerWhen {
  if (trigger.kind === `schedule`) {
    return {
      kind: `schedule`,
      interval: trigger.interval,
      minuteOfDay: trigger.minuteOfDay,
      ...(trigger.weekday !== undefined ? { weekday: trigger.weekday } : {}),
      ...(trigger.dayOfMonth !== undefined
        ? { dayOfMonth: trigger.dayOfMonth }
        : {}),
    }
  }
  return {
    kind: `event`,
    event: trigger.event,
    ...(trigger.filters ? { filters: trigger.filters } : {}),
  }
}

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

/** EXP-1029: a device's agent defaults as ONE flattened view — the shape the
 *  settings UI edits (EXP-1020). `model` / `subagentModel` are the last used
 *  agent's launch defaults, stored in `DeviceLaunchDefaults.agents[agent]`. */
export interface DeviceAgentDefaults {
  model: string
  subagentModel: string
  threadModel: string
}

/** Contract `deviceAgentDefaults`, hand-mirrored (drift-tested): what a
 *  device that never set anything is read as — the desktop's own fresh-
 *  install defaults (`coding::settings`, wired to the SAME contract
 *  constants). Every model is explicit: no blank = "CLI default". */
export const DEVICE_AGENT_DEFAULTS: DeviceAgentDefaults = {
  model: `fable`,
  subagentModel: `opus`,
  threadModel: `haiku`,
}

/** A dispatcher root's WORK picks (`sessions.work_launch`, server-only): what
 *  its children run with. The dispatcher itself runs on the thread model. */
export interface SessionWorkLaunch {
  model?: string | null
  effort?: string | null
  planMode?: boolean
  ultracode?: boolean
  computerUse?: boolean
}

/** EXP-1252 `sessions.handoff` (server-only): a pending move's request and,
 *  once the source uploaded it, the handoff package (worktree + transcript). */
export interface SessionHandoff {
  request?: {
    deviceId: string
    /** The login to use on the target (a profile id there), else carried. */
    account: string | null
    requestedBy: string
    requestedAt: string
  } | null
  blob?: {
    storageKey: string
    sizeBytes: number
    fromDeviceId: string
    /** The source device's name at upload ("Moved from …", the brief). */
    fromDeviceName?: string
    uploadedAt: string
  } | null
}

/** EXP-1252: the handoff package cap (gzip bytes). */
export const SESSION_HANDOFF_MAX_BYTES = 64 * 1024 * 1024


// ── Threads v2 (EXP-1291): ONE session tree ─────────────────────────────────
// A thread IS a root session (`sessions.kind = thread`, `parent_id` NULL); a
// team agent is a root of kind `agent` (never ends: it idles and wakes on a
// message or a trigger). Children are sub-sessions on any device (kind
// `thread`). Every start, message, follow-up, action run, trigger firing and
// agent spawn is ONE call: `sessions.send` (sessionsSendInputSchema below).
// The database is the bus: a new session is a `queued` row naming its device,
// a message a `messages` row; the device picks both up off its shapes.

export const sessionKindValues = [`thread`, `agent`] as const
export type SessionKind = (typeof sessionKindValues)[number]
export const sessionKindSchema = z.enum(sessionKindValues)

/** queued → running ⇄ idle → ended. A message to an idle session resumes the
 *  SAME row; agents never reach `ended`. */
export const sessionStatusValues = [`queued`, `running`, `idle`, `ended`] as const
export type SessionStatus = (typeof sessionStatusValues)[number]
export const sessionStatusSchema = z.enum(sessionStatusValues)

/** Who linked an issue to a thread (session_issues.source). */
export const sessionIssueSourceValues = [`user`, `agent`] as const
export type SessionIssueSource = (typeof sessionIssueSourceValues)[number]
export const sessionIssueSourceSchema = z.enum(sessionIssueSourceValues)

/** messages.author_kind: a person, the session's agent (its turn's final
 *  reply or a card), or the server (event cards). */
export const messageAuthorKindValues = [`user`, `agent`, `system`] as const
export type MessageAuthorKind = (typeof messageAuthorKindValues)[number]
export const messageAuthorKindSchema = z.enum(messageAuthorKindValues)

export const messageCardKindValues = [
  `question`,
  `plan`,
  `subagent`,
  `session`,
  `picture`,
  `event`,
  `thread_started`,
  `report`,
  `suggestions`,
] as const
export type MessageCardKind = (typeof messageCardKindValues)[number]

export const messageEventKindValues = [
  `issue_linked`,
  `issue_filed`,
  `issue_unlinked`,
  `pr_opened`,
  `pr_merged`,
  `pr_closed`,
  `session_ended`,
] as const
export type MessageEventKind = (typeof messageEventKindValues)[number]

/** EXP-1322: one option of a question card = its label (`options` is
 *  always `string[]`). An answer names the option by its label. The
 *  description of option i rides the parallel `optionDescriptions` sidecar
 *  (`questionCardOptionDescriptionsSchema`). */
export type QuestionCardOption = string

export const questionCardOptionSchema = z.string().max(500)

/** Cap on a question card's options (and so on its sidecar). */
export const QUESTION_CARD_OPTIONS_MAX = 50

/** EXP-1322 sidecar: `messages.card.optionDescriptions`, parallel to
 *  `options` (same length); null or "" = that option has no description.
 *  Omitted entirely when no option has a description. Readers resolve option
 *  i as `{label: options[i], description: optionDescriptions?.[i] || null}`. */
export type QuestionCardOptionDescriptions = (string | null)[]

export const questionCardOptionDescriptionsSchema = z
  .array(z.string().max(2000).nullable())
  .max(QUESTION_CARD_OPTIONS_MAX)

/** The question card as a device posts it (`sessions.reply`) and as it is
 *  read back: `options` = labels, `optionDescriptions` = the sidecar (must be
 *  parallel to `options` when present). Passthrough like `messageCardSchema`
 *  (`questionId` rides along). */
export const questionCardSchema = z
  .object({
    kind: z.literal(`question`),
    question: z.string(),
    options: z.array(questionCardOptionSchema).max(QUESTION_CARD_OPTIONS_MAX).optional(),
    optionDescriptions: questionCardOptionDescriptionsSchema.nullish(),
    answer: z.string().nullish(),
  })
  .passthrough()
  .refine(
    (card) =>
      card.optionDescriptions == null ||
      card.options === undefined ||
      card.optionDescriptions.length === card.options.length,
    { message: `optionDescriptions must be parallel to options`, path: [`optionDescriptions`] },
  )

/** messages.card: what a non-text conversation row shows. `session` = ONE
 *  child session's card (the Work card header row, EXP-1240; opens that
 *  session), `thread_started` = an agent channel's "started ▸ Thread" card,
 *  `question`/`plan` answer IN the card (`answer` + messages.answered_at),
 *  `picture` = a Guide picture the session showed while it worked. */
export type MessageCard =
  | {
      kind: `question`
      question: string
      options?: QuestionCardOption[]
      /** EXP-1322 sidecar, parallel to `options`; absent when no option has a description. */
      optionDescriptions?: QuestionCardOptionDescriptions | null
      answer?: string | null
    }
  | {
      kind: `plan`
      plan: string
      decision?: `approved` | `rejected` | null
      /** An approval that starts the work in a fresh context (ask-dock.json `planOptions`). */
      fresh?: boolean | null
      feedback?: string | null
    }
  | { kind: `subagent`; title: string; status: string; summary?: string | null }
  | { kind: `session`; sessionId: string }
  | { kind: `picture`; attachmentId: string; width: number | null; height: number | null; caption?: string | null }
  | {
      kind: `event`
      event: MessageEventKind
      issueId?: string | null
      pullRequestId?: string | null
      sessionId?: string | null
    }
  | { kind: `thread_started`; sessionId: string }
  | { kind: `report`; sessionId: string; summary: string }
  | { kind: `suggestions`; items: SuggestionItem[] }

/** EXP-1323: one item of a `suggestions` card (`sessions_suggest`). Start =
 *  a child (`sessionId` stamped), Do it here = a message to the posting
 *  session (`here` stamped), File as issue = `issueId` stamped. */
export interface SuggestionItem {
  id: string
  title: string
  text: string
  issueIds?: string[] | null
  sessionId?: string | null
  issueId?: string | null
  here?: boolean | null
  dismissed?: boolean | null
}

export const SUGGESTIONS_MAX = 10

export const suggestionInputSchema = z.object({
  title: z.string().trim().min(1).max(120),
  text: z.string().trim().min(1).max(4000),
  issueIds: z.array(z.string()).max(10).optional(),
})

// Tolerant read schema (a future card kind must not brick a collection):
// clients render unknown kinds as nothing. Stays passthrough: the EXP-1322
// `optionDescriptions` sidecar and `questionId` ride through unvalidated here
// (`questionCardSchema` validates a posted question card).
export const messageCardSchema = z
  .object({ kind: z.string().max(32) })
  .passthrough()

/** The Guide (ONE per thread, on the root): the PR body of every PR of the
 *  thread. Same entry shape and caps as the EXP-879/1154 results. */

// ── Agent triggers (agent roots only; SLOP-2's action triggers moved here) ──
// A trigger carries only its WHEN: the runner is the agent itself (its
// device, account, model). LOCAL-ONLY like before: the agent's device reads
// them off the sessions shape and posts the firing as a message into the
// agent's channel (`sessions.send({sessionId: agent})`).
export const MAX_AGENT_TRIGGERS = 10

export type AgentTrigger = { id: string; enabled: boolean } & (
  | ScheduleTriggerWhen
  | (EventTriggerWhen & { source: AgentTriggerSource })
)

export const agentTriggerSchema = z.discriminatedUnion(`kind`, [
  z
    .strictObject({
      id: z.string().uuid().optional(),
      enabled: z.boolean().default(true),
      ...scheduleTriggerShape,
    })
    .superRefine(refineSchedule),
  z
    .strictObject({
      id: z.string().uuid().optional(),
      enabled: z.boolean().default(true),
      ...eventTriggerShape,
    })
    .superRefine(refineEvent),
])
export const agentTriggersSchema = z.array(agentTriggerSchema).max(MAX_AGENT_TRIGGERS)

// ── sessions.send (THE entrypoint) ──────────────────────────────────────────
export const SESSIONS_SEND_TEXT_MAX = 16384
export const SESSIONS_SEND_MAX_ATTACHMENTS = 10
export const SESSIONS_SEND_MAX_ISSUES = 30
export const SESSION_BRIEF_MAX_BYTES = 65536
export const MESSAGE_BODY_MAX = 65536
export const AGENT_NAME_MAX = 64
export const AGENT_INSTRUCTIONS_MAX_BYTES = 16384
/** Follow-up trees: depth below a root, sessions per tree. */
export const SESSION_TREE_MAX_DEPTH = 3
export const SESSION_TREE_MAX_SESSIONS = 10

export const agentFacetsSchema = z.object({
  name: z.string().trim().min(1).max(AGENT_NAME_MAX),
  icon: boardIconSchema,
  color: hexColorSchema,
  instructions: z.string().max(AGENT_INSTRUCTIONS_MAX_BYTES).default(``),
  triggers: agentTriggersSchema.default([]),
})
export type AgentFacetsInput = z.infer<typeof agentFacetsSchema>

const sendUuid = z.string().uuid()

/**
 * `sessions.send`: message an existing session (`sessionId`; resumes it when
 * idle, an ended THREAD continues in a new child), start a CHILD
 * (`parentId`), or start a new ROOT (neither): a thread (optionally with
 * `issueIds` / `actionId`+`inputs` / `repositoryId`) or, with `agent`, a team
 * agent. EXP-1341: `name` names a NEW root or child (omitted = it names
 * itself, session-title.json); `issueIds` on a child ASSIGNS them to it, on
 * a message FORWARDS them to that session (linked to the root first).
 * A new session needs `deviceId` (the composer's device picker; an
 * agent's spawn defaults to the agent's own device). `teamId` = the ACTIVE
 * team; omitted, it resolves from `sessionId`/`parentId`, else the CALLING
 * session's team (MCP), else it is required.
 */
export const sessionsSendInputSchema = z
  .object({
    teamId: sendUuid.optional(),
    sessionId: sendUuid.optional(),
    parentId: sendUuid.optional(),
    // Runner pickers (new sessions only), except `account`: on a message
    // (`sessionId`) it SWITCHES the session's login (EXP-849, claude only;
    // the host device resumes the same row on it between turns) and may be
    // sent alone, with no text. EXP-1252: `deviceId` on a message = MOVE the
    // session to that device after its turn (its current device = cancel a
    // pending move); may be sent alone, `account` = the login to use there.
    deviceId: z.string().min(1).max(128).optional(),
    cli: z.string().min(1).max(16).optional(),
    account: z.string().min(1).max(64).optional(),
    model: z.string().max(64).optional(),
    effort: z.string().max(32).optional(),
    planMode: z.boolean().optional(),
    computerUse: z.boolean().optional(),
    ultracode: z.boolean().optional(),
    mcpServerIds: z.array(sendUuid).max(20).optional(),
    // The message.
    text: z
      .string()
      .max(SESSIONS_SEND_TEXT_MAX)
      .refine((value) => !value.includes(`\u0000`), `contains NUL bytes`)
      .optional(),
    attachmentIds: z.array(sendUuid).max(SESSIONS_SEND_MAX_ATTACHMENTS).optional(),
    // EXP-1341: the new session's display name (root or child).
    name: z.string().trim().min(1).max(255).optional(),
    // A new root's issues; a child's ASSIGNED issues; a message's FORWARDED ones.
    issueIds: z.array(sendUuid).min(1).max(SESSIONS_SEND_MAX_ISSUES).optional(),
    actionId: sendUuid.optional(),
    inputs: z.record(z.string(), z.string().max(MAX_ACTION_INPUT_TEXT)).optional(),
    repositoryId: sendUuid.optional(),
    // A NEW team agent root (Settings → Agents).
    agent: agentFacetsSchema.optional(),
    // EXP-1330: a `/btw` aside child of `parentId` (text = the question).
    aside: z.boolean().optional(),
  })
  .superRefine((input, ctx) => {
    const issue = (message: string) =>
      ctx.addIssue({ code: z.ZodIssueCode.custom, message })
    const rootFacets =
      input.actionId !== undefined ||
      input.repositoryId !== undefined ||
      input.agent !== undefined
    if (input.sessionId) {
      if (input.parentId) issue(`sessionId and parentId are exclusive`)
      if (input.aside) issue(`an aside needs a parentId and a question`)
      if (rootFacets) issue(`a message to a session takes no root facets`)
      if (input.name) issue(`a name is for a new session`)
      if (
        !input.text?.trim() &&
        !input.attachmentIds?.length &&
        !input.account &&
        !input.issueIds &&
        !input.deviceId
      )
        issue(`a message needs text, attachments, issues, an account or a device`)
      return
    }
    if (input.aside && (!input.parentId || !input.text?.trim()))
      issue(`an aside needs a parentId and a question`)
    if (input.parentId && rootFacets) issue(`a child session takes no root facets`)
    if (input.agent && (input.issueIds || input.actionId))
      issue(`an agent takes no issues or action`)
    if (input.agent && input.name) issue(`an agent is named by agent.name`)
    if (input.aside && input.issueIds) issue(`an aside takes no issues`)
    if (input.inputs && !input.actionId) issue(`inputs need an actionId`)
    if (!input.agent && !input.text?.trim() && !input.issueIds && !input.actionId)
      issue(`a new session needs text, issues or an action`)
  })
export type SessionsSendInput = z.infer<typeof sessionsSendInputSchema>

/**
 * EXP-1334 `sessions.configure`: change a LIVE session's launch options
 * mid-session (same steer rule as messaging it; an ended session refuses).
 * Omitted = unchanged; `effort: null`/`""` = the agent's default. The host
 * device applies them: plan mode and the model at once, everything else at
 * the next turn (a between-turn restart resuming the SAME run). On a
 * dispatcher root the picks are its WORK picks (its children's).
 */
export const sessionsConfigureInputSchema = z
  .object({
    sessionId: sendUuid,
    // The agent's model (contract codingModel / codexModel): the live run
    // switches at once (`/model`), a resume launches on it.
    model: z.string().min(1).max(64).optional(),
    planMode: z.boolean().optional(),
    effort: z.string().max(32).nullable().optional(),
    ultracode: z.boolean().optional(),
    computerUse: z.boolean().optional(),
    mcpServerIds: z.array(sendUuid).max(20).optional(),
  })
  .refine(
    (input) =>
      input.model !== undefined ||
      input.planMode !== undefined ||
      input.effort !== undefined ||
      input.ultracode !== undefined ||
      input.computerUse !== undefined ||
      input.mcpServerIds !== undefined,
    `nothing to configure`
  )
export type SessionsConfigureInput = z.infer<typeof sessionsConfigureInputSchema>
export interface SessionsSendResult {
  sessionId: string
  /** The `messages` row the text became; null for a start without text. */
  messageId: string | null
  /** The session's status at return. A new session is `queued` = PENDING:
   *  the call never waits for the device, which picks the row up off the
   *  sessions shape (FEED-101: a start is never reported as failed while
   *  its device is still on the way). */
  status: SessionStatus
}

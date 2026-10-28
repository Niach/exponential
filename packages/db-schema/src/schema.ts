import {
  type AnyPgColumn,
  bigint,
  doublePrecision,
  index,
  integer,
  jsonb,
  pgEnum,
  pgTable,
  primaryKey,
  text,
  timestamp,
  unique,
  uniqueIndex,
  uuid,
  check,
  varchar,
  boolean,
  date,
} from "drizzle-orm/pg-core"
import { sql, type InferSelectModel } from "drizzle-orm"
import { createSchemaFactory } from "drizzle-zod"
import { z } from "zod"
import {
  type ActionInputDef,
  actionInputsSchema,
  type CodingSessionBlocked,
  type AgentTrigger,
  type AgentTriggerState,
  type EventEmitter,
  eventEmitterValues,
  type GuideEntry,
  type MessageCard,
  messageAuthorKindSchema,
  messageAuthorKindValues,
  conversationKindValues,
  conversationKindSchema,
  type ConversationKind,
  sessionIssueSourceSchema,
  sessionIssueSourceValues,
  sessionKindSchema,
  sessionKindValues,
  sessionStatusSchema,
  sessionStatusValues,
  commentBodyWithAttachmentsSchema,
  commentAudienceValues,
  commentSourceValues,
  issueDescriptionSchema,
  issueEventTypeSchema,
  issueEventTypeValues,
  issueRelationTypeValues,
  issueRelationSourceValues,
  issueRelationTypeSchema,
  issueRelationSourceSchema,
  issuePrioritySchema,
  issuePriorityValues,
  issueSourceSchema,
  issueSourceValues,
  issueStatusCategorySchema,
  issueStatusCategoryValues,
  issueStatusSchema,
  type IssueStatus,
  mcpAuthSchema,
  mcpTransportSchema,
  type NotificationType,
  notificationTypeValues,
  prStateSchema,
  prStateValues,
  pinKindSchema,
  pinKindValues,
  subscriberSourceSchema,
  subscriberSourceValues,
  teamRoleSchema,
  teamRoleValues,
} from "./domain"

export * from "./auth-schema"
import { users, oauthApplications } from "./auth-schema"

const { createInsertSchema, createSelectSchema } = createSchemaFactory({
  zodInstance: z,
})

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

// The PG type keeps the orphan `todo` label (EXP-685 migrated every Todo issue
// to Backlog, migration 0091): dropping an enum VALUE needs a full type
// recreate, and no writer or reader can produce it any more. Deliberately NOT
// derived from issueStatusValues — that list is the app vocabulary.
export const issueStatusEnum = pgEnum(`issue_status`, [
  `backlog`,
  `todo`,
  `in_progress`,
  `in_review`,
  `done`,
  `cancelled`,
  `duplicate`,
])

// EXP-314: the fixed category a custom/builtin issue status belongs to.
export const issueStatusCategoryEnum = pgEnum(
  `issue_status_category`,
  issueStatusCategoryValues
)

export const issuePriorityEnum = pgEnum(`issue_priority`, issuePriorityValues)

export const issueSourceEnum = pgEnum(`issue_source`, issueSourceValues)

export const notificationTypeEnum = pgEnum(
  `notification_type`,
  notificationTypeValues
)

export const teamMemberRoleEnum = pgEnum(`team_member_role`, teamRoleValues)

export const prStateEnum = pgEnum(`pr_state`, prStateValues)


export const issueEventTypeEnum = pgEnum(
  `issue_event_type`,
  issueEventTypeValues
)

export const subscriberSourceEnum = pgEnum(
  `subscriber_source`,
  subscriberSourceValues
)

// EXP-778: what a personal pin points at.
export const pinKindEnum = pgEnum(`pin_kind`, pinKindValues)

export const issueRelationTypeEnum = pgEnum(
  `issue_relation_type`,
  issueRelationTypeValues
)

export const issueRelationSourceEnum = pgEnum(
  `issue_relation_source`,
  issueRelationSourceValues
)


// EXP-1291 Threads v2: the session tree's vocabulary.
export const sessionKindEnum = pgEnum(`session_kind`, sessionKindValues)
export const sessionStatusEnum = pgEnum(`session_status`, sessionStatusValues)
export const sessionIssueSourceEnum = pgEnum(
  `session_issue_source`,
  sessionIssueSourceValues
)
export const messageAuthorKindEnum = pgEnum(
  `message_author_kind`,
  messageAuthorKindValues
)
// NEW-3: conversations.kind.
export const conversationKindEnum = pgEnum(`conversation_kind`, conversationKindValues)

// EXP-741: who posted a comment (a person, or an agent over MCP).
export const commentSourceEnum = pgEnum(`comment_source`, commentSourceValues)
export const commentAudienceEnum = pgEnum(
  `comment_audience`,
  commentAudienceValues
)

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const uuidPk = () =>
  uuid()
    .primaryKey()
    .default(sql`gen_random_uuid()`)

const timestamps = {
  createdAt: timestamp(`created_at`, { withTimezone: true })
    .notNull()
    .defaultNow(),
  updatedAt: timestamp(`updated_at`, { withTimezone: true })
    .notNull()
    .defaultNow(),
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

// Teams are ALWAYS private: membership is invite-only and nothing in a
// team is ever anonymously readable (EXP-180 removed the public feedback
// boards that used to be the one exception).
export const teams = pgTable(`teams`, {
  id: uuidPk(),
  name: varchar({ length: 255 }).notNull(),
  slug: varchar({ length: 255 }).notNull().unique(),
  iconUrl: text(`icon_url`),
  // Admin-granted complimentary tier ('pro' | 'business' | 'unlimited').
  // SERVER-ONLY — must stay behind the teams shape columns allowlist.
  // Honored by getTeamPlan as a floor over the Creem-derived tier.
  compTier: text(`comp_tier`),
  // EXP-319 — per-team PR automation targets. NULL status_id = the builtin
  // default (In Review on open, Done on merge) — deliberately NOT "do
  // nothing", so the FK's SET NULL (target status deleted) falls back to the
  // builtin instead of silently disabling the automation. "Do nothing" is the
  // explicit *_automation=false flag. Synced through the teams shape so
  // clients can render the automation hint.
  // The `AnyPgColumn` annotations break the teams ↔ issue_statuses circular
  // type inference (issue_statuses.team_id references teams).
  prOpenedStatusId: uuid(`pr_opened_status_id`).references(
    (): AnyPgColumn => issueStatuses.id,
    { onDelete: `set null` }
  ),
  prOpenedAutomation: boolean(`pr_opened_automation`).notNull().default(true),
  prMergedStatusId: uuid(`pr_merged_status_id`).references(
    (): AnyPgColumn => issueStatuses.id,
    { onDelete: `set null` }
  ),
  prMergedAutomation: boolean(`pr_merged_automation`).notNull().default(true),
  // EXP-711 — does a merged PR END the live coding sessions on its issues
  // (EXP-498's default)? false keeps them running; the merging run's own
  // spare (`coding_sessions.merged_own_pr`) and MCP `pr_merge`'s per-call
  // `endSessions` override sit on top. Synced so every client renders the
  // toggle and the desktop's batch self-close honours it.
  endSessionsOnMerge: boolean(`end_sessions_on_merge`).notNull().default(true),
  // EXP-1105 — yolo mode (owner toggle, default off): every PR an agent opens
  // via MCP `pr_open` squash-merges at once, and every client hides Reviews
  // (desktop also Files + Source Control) unless something failed (an open PR
  // left behind, a trunk-sync conflict). Synced so the nav can gate.
  yoloMode: boolean(`yolo_mode`).notNull().default(false),
  // EXP-630 — the estimate scale (contract `issueEstimation`, a documented
  // varchar): `none` (default) hides estimates everywhere; the other values
  // pick the ladder the picker offers and how a value reads (t-shirt).
  // Synced so every client renders the chip the same way.
  estimationType: varchar(`estimation_type`, { length: 16 }).notNull().default(`none`),
  // EXP-1025 — the TEAM PROMPT: owner-authored markdown every coding run of
  // the team gets appended to its system prompt, after the run playbook and
  // before the repo's CLAUDE.md (rebuilt on every start AND resume). Capped
  // at contract `team.agentPromptMaxBytes`. SERVER-ONLY like `actions.body`:
  // behind the teams shape allowlist, read through `teams.getAgentPrompt`
  // (members) and written through `teams.update` (owners); the launcher
  // fetches it at prepare time. `agent_prompt_updated_at` = the last write
  // whose normalized text DIFFERED from the stored one (`teams.update` reads
  // the row and skips the stamp otherwise), for the editors' "edited …"
  // caption.
  agentPrompt: text(`agent_prompt`).notNull().default(``),
  agentPromptUpdatedAt: timestamp(`agent_prompt_updated_at`, { withTimezone: true }),
  ...timestamps,
})

// Better Auth's Drizzle adapter resolves models by snake_case key, so this
// must be exported as `creem_subscriptions` (not camelCase). It lives here
// (rather than in auth-schema.ts) so its `team_id` FK can reference
// `teams` locally — auth-schema.ts must NOT import schema.ts (that edge
// forms an eval-time circular import that crashes `createSelectSchema`).
export const creem_subscriptions = pgTable(
  `creem_subscriptions`,
  {
    id: text(`id`).primaryKey(),
    productId: text(`product_id`).notNull(),
    // The BUYER (the Creem customer whose card is charged). NULLABLE + `set
    // null` on purpose (REV2-55): a subscription belongs to the TEAM, not to
    // the person who happened to pay for it, so it must survive the purchaser
    // deleting their account — the old `cascade` silently destroyed a surviving
    // team's billing row (and with it the local billing history) the moment the
    // buyer left. Remaining owners keep managing the subscription through the
    // team-scoped billing router; `reference_id` is only the buyer attribution
    // used by getUserPlan's free-tier owned-team guard.
    referenceId: text(`reference_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    creemCustomerId: text(`creem_customer_id`),
    // ONE row per Creem subscription, and the key never changes once set:
    // the unique index below plus the reject_creem_subscription_rekey trigger
    // (REV-12). Without the trigger, the Creem plugin's webhook persistence
    // could RE-KEY an existing row — its subscription-event fallback matches
    // by creem_customer_id when the id lookup misses and then writes the new
    // id onto whatever row it found, silently merging two paying
    // subscriptions into one local row.
    creemSubscriptionId: text(`creem_subscription_id`),
    creemOrderId: text(`creem_order_id`),
    // v5 per-seat binding: a subscription belongs to exactly one team, and
    // `seats` is the purchased quantity (Creem checkout `units`). Both are
    // bound from checkout metadata on the webhook path
    // (lib/billing/creem-binding.ts); the plugin's own persistence never
    // writes these columns, and the re-key protections above keep a later
    // webhook update from re-pointing the row's KEY out from under the
    // binding. `set null` keeps the billing history row if the team is
    // deleted.
    teamId: uuid(`team_id`).references(() => teams.id, {
      onDelete: `set null`,
    }),
    seats: integer(`seats`).default(1).notNull(),
    status: text(`status`)
      .$defaultFn(() => `pending`)
      .notNull(),
    periodStart: timestamp(`period_start`),
    periodEnd: timestamp(`period_end`),
    cancelAtPeriodEnd: boolean(`cancel_at_period_end`)
      .$defaultFn(() => false)
      .notNull(),
    createdAt: timestamp(`created_at`)
      .$defaultFn(() => /* @__PURE__ */ new Date())
      .notNull(),
    updatedAt: timestamp(`updated_at`)
      .$defaultFn(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex(`uniq_creem_subscriptions_creem_subscription_id`).on(
      table.creemSubscriptionId
    ),
  ]
)

export const teamMembers = pgTable(
  `team_members`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    role: teamMemberRoleEnum().notNull().default(`member`),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.userId),
    index(`idx_team_members_user`).on(table.userId),
  ]
)

export const teamInvites = pgTable(
  `team_invites`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    invitedById: text(`invited_by_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    role: teamMemberRoleEnum().notNull().default(`member`),
    token: varchar({ length: 255 }).notNull().unique(),
    // Optional recipient address (EXP-188 invite-by-email). Display metadata
    // only — accept() stays token-bound, never recipient-bound.
    email: varchar({ length: 255 }),
    // EXP-630: the placeholder member this email invite created up front
    // (users.placeholder_at) — already on the roster, assignable and
    // attributable before the person joins. accept() claims it: the accepting
    // account IS the placeholder (signed in through its email) or absorbs it
    // (attributions move to the accepter, the placeholder row goes).
    // Cascades: revoking keeps the row, deleting the placeholder drops the
    // invite. Synced (team-invites shape) so member lists can badge the row.
    placeholderUserId: text(`placeholder_user_id`).references(
      () => users.id,
      { onDelete: `cascade` }
    ),
    acceptedAt: timestamp(`accepted_at`, { withTimezone: true }),
    // EXP-1076: when the invite LINK was issued (mail attempted, or the token
    // handed to the owner). NULL = the roster row exists but nobody was ever
    // invited — the import's placeholder members (Settings → Members shows
    // "Not invited" with a "Send invite" action). Such rows are also stamped
    // `expires_at = created_at`, so every expiry-based reader (accept, seat
    // counts, pending lists) already treats them as dead.
    sentAt: timestamp(`sent_at`, { withTimezone: true }),
    expiresAt: timestamp(`expires_at`, { withTimezone: true }).notNull(),
    ...timestamps,
  },
  (table) => [
    index(`idx_team_invites_team`).on(table.teamId),
    index(`idx_team_invites_placeholder_user`).on(table.placeholderUserId),
  ]
)

export const boards = pgTable(
  `boards`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    slug: varchar({ length: 255 }).notNull(),
    // Uppercase, letter-led, max 4 chars (REV-4 — was 10; migration 0069
    // truncated longer ones). Unique per team so `{PREFIX}-{number}`
    // identifiers stay team-unique — reviews/mentions/branches resolve
    // issues by bare identifier.
    prefix: varchar({ length: 4 }).notNull(),
    color: varchar({ length: 7 }).notNull().default(`#6366f1`),
    // Curated display icon (boardIconValues in domain.ts / the domain
    // contract). NULL = clients derive a fallback from repo presence.
    icon: text(),
    // A repo-backed board is backed by exactly one repo from the team
    // registry; the desktop launcher clones this. Nullable: boards need no
    // repo. `restrict` (not cascade): a repo that still backs a board can't
    // be deleted — retarget or delete the boards first. One repo may back
    // several boards (monorepo); plan limits still count registry rows.
    repositoryId: uuid(`repository_id`).references(() => repositories.id, {
      onDelete: `restrict`,
    }),
    // EXP-712: the branch THIS board's coding sessions branch from and its PRs
    // target. NULL = follow the repo (its team-pinned `default_branch_override`,
    // else GitHub's default). Lets two boards on one repo develop on
    // different branches (release/1.x vs main). Synced (boards shape); reset
    // to NULL whenever the board is retargeted to another repo.
    defaultBranch: text(`default_branch`),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    // Soft-delete (trash) marker. Non-null = trashed; the purge sweep hard-deletes
    // it (cascade) once deletedAt + BOARD_TRASH_RETENTION_MS has passed. Purge
    // time is computed, never stored (constant retention). Trashed boards drop
    // out of every membership/public scope but keep their rows for restore.
    deletedAt: timestamp(`deleted_at`, { withTimezone: true }),
    // Archive marker (EXP-500) — the non-purging sibling of deletedAt. Non-null
    // = archived: the board and every one of its issues drop out of the Electric
    // shapes and of every server read surface, exactly like the trash, but
    // NOTHING ever deletes them. Owners bring a board back with boards.unarchive.
    // Archiving previously shipped as a SYNCED column each client filtered by
    // hand and was deleted for leaking (REV2-103); it is server-side now.
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.slug),
    // Duplicate prefixes minted colliding identifiers (two boards' WEB-7);
    // like the slug, a trashed board keeps its prefix reserved until purge.
    unique().on(table.teamId, table.prefix),
    index(`idx_boards_repository`).on(table.repositoryId),
    // Serves the purge sweep + trash-aware shape filter; near-empty in steady
    // state (only trashed rows are indexed).
    index(`idx_boards_deleted`)
      .on(table.deletedAt)
      .where(sql`deleted_at IS NOT NULL`),
    // Serves boards.listArchived; near-empty in steady state, like the trash
    // index above (only archived rows are indexed).
    index(`idx_boards_archived`)
      .on(table.archivedAt)
      .where(sql`archived_at IS NOT NULL`),
  ]
)

export const issues = pgTable(
  `issues`,
  {
    id: uuidPk(),
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    // Trigger-denormalized from the board (populate_issue_board_context —
    // writers pass the value they already resolved for auth, the trigger
    // overwrites with board-derived truth) so the issues Electric shape can be
    // TEAM-scoped: a per-user board-id where clause rotated the shape identity
    // on every board create/trash in ANY of the user's teams (REV2-5).
    // Server-only — excluded from the shape via its columns allowlist (native
    // schemas don't carry it).
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Mirrors of the parent board's deleted_at / archived_at, maintained by
    // populate_issue_board_context on insert/move and fanned out by
    // propagate_board_deleted_at / propagate_board_archived_at on
    // trash/restore and archive/unarchive. Lets the shape's hide filters be
    // the STATIC predicates `board_deleted_at IS NULL` +
    // `board_archived_at IS NULL` instead of a per-request board-id list
    // (REV2-5, EXP-500). Server-only, shape-excluded.
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    number: integer().notNull().default(0),
    identifier: varchar({ length: 20 }).notNull().default(``),
    title: varchar({ length: 500 }).notNull(),
    // Plain GFM markdown (was jsonb `{ text }`).
    description: text(),
    // Narrowed to the app vocabulary: the PG type also carries the orphan
    // `todo` label (EXP-685) that no row holds any more.
    status: issueStatusEnum().$type<IssueStatus>().notNull().default(`backlog`),
    // EXP-314: the precise per-team status row; `status` above stays the
    // dual-written builtin anchor. NULLABLE + SET NULL so a status delete or
    // team cascade can never wedge an issue; the populate_issue_status_id
    // trigger derives it for enum-only writers and clients fall back to the
    // anchor when it's NULL/unresolvable.
    statusId: uuid(`status_id`).references(() => issueStatuses.id, {
      onDelete: `set null`,
    }),
    priority: issuePriorityEnum().notNull().default(`none`),
    assigneeId: text(`assignee_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    // #EXP-1383: an issue assigned to a team AGENT instead of a person.
    // Mutually exclusive with `assignee_id` (issues_single_assignee_check).
    // SYNCED.
    assigneeAgentId: uuid(`assignee_agent_id`).references(
      (): AnyPgColumn => agents.id,
      { onDelete: `set null` }
    ),
    // NULLABLE: widget-filed feedback issues have no user creator (EXP: the
    // synthetic per-widget bot user was removed). `set null` (not cascade):
    // deleting a user now leaves their authored issues in place with a null
    // creator instead of erasing them.
    creatorId: text(`creator_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    // Where the issue came from: `user` (a signed-in member filed it — the
    // default) or `widget` (filed anonymously through the embeddable feedback
    // widget; pairs with a null creator_id). Clients key the "Feedback widget"
    // origin off this.
    source: issueSourceEnum().notNull().default(`user`),
    // #EXP-1383: the agent whose session created the issue (`creator_id`
    // keeps the member behind it). SYNCED.
    creatorAgentId: uuid(`creator_agent_id`).references(
      (): AnyPgColumn => agents.id,
      { onDelete: `set null` }
    ),
    dueDate: date(`due_date`),
    // EXP-630: story points. A plain non-negative integer (Linear's
    // fibonacci/linear/t-shirt scales all import as their point value);
    // NULL = not estimated. SYNCED (appended to the issues shape allowlist).
    estimate: integer(),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    completedAt: timestamp(`completed_at`, { withTimezone: true }),
    // Duplicate resolution: this issue is a duplicate of the canonical issue.
    // 1:1 (no relation graph); pairs with status='duplicate'.
    duplicateOfId: uuid(`duplicate_of_id`).references(
      (): AnyPgColumn => issues.id,
      { onDelete: `set null` }
    ),
    // EXP-1231: the issue DRAFT this issue was created from (`issues.create
    // ({ draftId })` consumed that `issue_drafts` row in the same
    // transaction). No FK: the draft row is gone by the time this is
    // written. SYNCED, so a draft page still open on another client sees
    // "created elsewhere" and lands on this issue instead of resurrecting
    // the draft; `issueDrafts.upsert` refuses a consumed id.
    draftId: uuid(`draft_id`),
    ...timestamps,
  },
  (table) => [
    index(`idx_issues_board_status`).on(table.boardId, table.status),
    // EXP-1231: "was this draft already created?" — the upsert guard and
    // the clients' created-elsewhere lookup. Partial — most issues have none.
    index(`idx_issues_draft_id`)
      .on(table.draftId)
      .where(sql`draft_id IS NOT NULL`),
    index(`idx_issues_team`).on(table.teamId),
    index(`idx_issues_assignee`).on(table.assigneeId),
    index(`idx_issues_assignee_agent`)
      .on(table.assigneeAgentId)
      .where(sql`assignee_agent_id IS NOT NULL`),
    check(
      `issues_single_assignee_check`,
      sql`assignee_id IS NULL OR assignee_agent_id IS NULL`
    ),
    index(`idx_issues_creator`).on(table.creatorId),
    index(`idx_issues_due_date`).on(table.dueDate),
    // The duplicate_of_id SET NULL RI trigger fires on every issue delete;
    // without this it seq-scans issues per deleted row inside the cascade.
    index(`idx_issues_duplicate_of`)
      .on(table.duplicateOfId)
      .where(sql`duplicate_of_id IS NOT NULL`),
    // Serves the status_id FK's SET NULL sweep and the statuses.delete
    // reassignment scan (same rationale as idx_issues_duplicate_of).
    index(`idx_issues_status_id`)
      .on(table.statusId)
      .where(sql`status_id IS NOT NULL`),
    // Backstop under generate_issue_number()'s counter allocator (see
    // issue_number_counters below): any residual allocation race fails loudly
    // instead of committing two issues with the same identifier.
    uniqueIndex(`uniq_issues_board_number`).on(table.boardId, table.number),
    // Serves issues.search's FTS branch (REV-14). The expression must stay
    // byte-identical to the query's tsvector expression in
    // apps/web/src/lib/issue-search-sql.ts (ISSUE_TSVECTOR) —
    // Postgres matches index and predicate on the parse tree, and 'english'
    // is pinned because the session default config is not immutable.
    index(`idx_issues_fts`).using(
      `gin`,
      sql`to_tsvector('english', coalesce(${table.title}, '') || ' ' || coalesce(${table.description}, ''))`
    ),
  ]
)

// Per-board monotonic issue-number allocator — server-only, NEVER
// Electric-synced (no shape proxy). The
// generate_issue_number() trigger (custom trigger file, re-applied at every
// boot by bootstrap-cloud applyCustomSql) increments this row under its row
// lock: serializes concurrent inserts (no duplicate numbers) and never
// decreases (deleting the top-numbered issue can't recycle its identifier —
// #PREFIX-n mentions and exp/PREFIX-n branches stay unambiguous). Keyed 1:1 by
// board on purpose (deliberate deviation from the uuid-surrogate-PK
// convention). No zod/insert-schema exports — no TS code queries it; only the
// trigger touches it.
export const issueNumberCounters = pgTable(`issue_number_counters`, {
  boardId: uuid(`board_id`)
    .primaryKey()
    .references(() => boards.id, { onDelete: `cascade` }),
  counter: integer().notNull(),
  ...timestamps,
})

export const labels = pgTable(
  `labels`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    color: varchar({ length: 7 }).notNull().default(`#6366f1`),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    index(`idx_labels_team`).on(table.teamId),
    // One label per (team, name), case-insensitive — labels.create/update
    // pre-check and map its violation to a readable CONFLICT (EXP-254).
    uniqueIndex(`uniq_labels_team_name_ci`).on(
      table.teamId,
      sql`lower(${table.name})`
    ),
  ]
)

// EXP-314 — per-team custom issue statuses (Linear-style), grouped into the
// fixed issue_status_category set. Every team carries 6 locked builtin rows
// (builtin_key != NULL, seeded by the seed_builtin_issue_statuses trigger and
// the migration backfill; values mirror contract.json issueStatusDefaults) —
// not renamable, not recolorable, not deletable; customs (builtin_key NULL)
// are member-managed. `issues.status_id` points here while `issues.status`
// keeps the dual-written builtin ANCHOR (CATEGORY_ANCHOR in domain.ts) so
// every enum-keyed subsystem and old client keeps working.
export const issueStatuses = pgTable(
  `issue_statuses`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Immutable after create (router-enforced — no update path accepts it).
    category: issueStatusCategoryEnum().notNull(),
    name: varchar({ length: 255 }).notNull(),
    color: varchar({ length: 7 }).notNull(),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    // NULL = custom status; non-null marks the locked builtin row for that
    // enum value (at most one per team).
    builtinKey: issueStatusEnum(`builtin_key`).$type<IssueStatus>(),
    ...timestamps,
  },
  (table) => [
    index(`idx_issue_statuses_team`).on(table.teamId),
    // One status per (team, name), case-insensitive — statuses.create/update
    // pre-check and map its violation to a readable CONFLICT (labels pattern).
    uniqueIndex(`uniq_issue_statuses_team_name_ci`).on(
      table.teamId,
      sql`lower(${table.name})`
    ),
    uniqueIndex(`uniq_issue_statuses_team_builtin`)
      .on(table.teamId, table.builtinKey)
      .where(sql`builtin_key IS NOT NULL`),
  ]
)

export const issueLabels = pgTable(
  `issue_labels`,
  {
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    labelId: uuid(`label_id`)
      .notNull()
      .references(() => labels.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Denormalized from issue→board by populate_issue_child_board_id so the
    // trash fan-out can target a board's child rows (Electric where clauses
    // are single-table).
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    // Mirrors of the parent board's deleted_at / archived_at
    // (trigger-maintained, REV2-5 + EXP-500) — the shape's static trash and
    // archive predicates. Server-only, shape-excluded.
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
  },
  (table) => [
    primaryKey({ columns: [table.issueId, table.labelId] }),
    index(`idx_issue_labels_label`).on(table.labelId),
    index(`idx_issue_labels_team`).on(table.teamId),
    index(`idx_issue_labels_board`).on(table.boardId),
  ]
)

// EXP-736 issue relations. One row per (issue, related, type) in the
// CANONICAL direction (see domain.ts issueRelationTypeValues); the source
// column is named `issue_id` so the shared populate_issue_child_* triggers
// derive team/board + the trash/archive mirrors from the SOURCE issue's board
// unchanged. Synced as the 20th shape (mirrors excluded by the allowlist).
export const issueRelations = pgTable(
  `issue_relations`,
  {
    id: uuidPk(),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    relatedIssueId: uuid(`related_issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    type: issueRelationTypeEnum().notNull(),
    source: issueRelationSourceEnum().notNull().default(`user`),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    uniqueIndex(`uniq_issue_relations_pair_type`).on(
      table.issueId,
      table.relatedIssueId,
      table.type
    ),
    index(`idx_issue_relations_related`).on(table.relatedIssueId),
    index(`idx_issue_relations_team`).on(table.teamId),
    index(`idx_issue_relations_board`).on(table.boardId),
    check(
      `chk_issue_relations_not_self`,
      sql`${table.issueId} <> ${table.relatedIssueId}`
    ),
  ]
)

export const comments = pgTable(
  `comments`,
  {
    id: uuidPk(),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Denormalized from issue→board (populate_issue_child_board_id) so the
    // trash fan-out can target a board's child rows.
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    // Mirrors of the parent board's deleted_at / archived_at
    // (trigger-maintained, REV2-5 + EXP-500) — the shape's static trash and
    // archive predicates. Server-only, shape-excluded.
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    // SLOP-4: NULLABLE — a widget reporter's words (`source = reporter`)
    // have no users row behind them; the card names the submission's
    // reporter (widgets.submissionForIssue) or "Anonymous visitor".
    authorId: text(`author_id`).references(() => users.id, {
      onDelete: `cascade`,
    }),
    // EXP-741: the top-level comment this one replies to — ONE level deep
    // (`comments.create` flattens a reply-to-a-reply onto the root), same
    // issue. NULL = a top-level comment. SET NULL, never cascade: deleting a
    // root must not delete a teammate's reply (EXP-398 — nobody deletes
    // someone else's words), and the same goes for the account-deletion
    // cascade off `author_id`. An orphaned reply FLATTENS to a top-level card:
    // every client's thread helper already renders a reply whose parent is
    // missing as top-level (lib/comment-threads.ts, iOS CommentThreads,
    // Android TimelineCollapse, desktop timeline.rs).
    parentId: uuid(`parent_id`).references((): AnyPgColumn => comments.id, {
      onDelete: `set null`,
    }),
    // EXP-741: `mcp` when an agent posted it over MCP (the card header shows
    // "via MCP"); stamped server-side from the MCP context, never by input.
    // SLOP-4: `reporter` when the widget reporter wrote it through their
    // magic-link page (author_id NULL, body server-escaped GFM).
    source: commentSourceEnum().notNull().default(`user`),
    // #EXP-1383: the agent whose session wrote it (`author_id` keeps the
    // member behind it); the card names the agent. SYNCED.
    authorAgentId: uuid(`author_agent_id`).references(
      (): AnyPgColumn => agents.id,
      { onDelete: `set null` }
    ),
    // SLOP-4: who may read it. `team` (default) never leaves the team;
    // `reporter` shows on the reporter's magic-link page and, from a member,
    // is emailed to the submission's reporter address. Reporter-written rows
    // are always `reporter`. Synced (every member sees both).
    audience: commentAudienceEnum().notNull().default(`team`),
    // SLOP-4: the outbound email that carried a member's reporter-audience
    // reply (audit; NULL for team comments, reporter rows and no-transport
    // sends). Server-only, shape-excluded.
    emailDeliveryId: uuid(`email_delivery_id`).references(
      () => emailDeliveries.id,
      { onDelete: `set null` }
    ),
    // Plain GFM markdown (was jsonb `{ text }`).
    body: text().notNull(),
    editedAt: timestamp(`edited_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_comments_issue`).on(table.issueId),
    index(`idx_comments_team`).on(table.teamId),
    index(`idx_comments_board`).on(table.boardId),
    index(`idx_comments_parent`).on(table.parentId),
    // Serves issues.search's comment-body FTS branch (REV-14) — its twin
    // lives in apps/web/src/lib/issue-search-sql.ts, not trpc/issues.ts. Same
    // byte-identical-expression contract as idx_issues_fts.
    index(`idx_comments_body_fts`).using(
      `gin`,
      sql`to_tsvector('english', ${table.body})`
    ),
  ]
)


export const attachments = pgTable(
  `attachments`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // NULLABLE since EXP-878: a row is owned by EITHER an issue OR an issue
    // draft (`attachments_owner_check`). Draft rows (`draft_id` set, issue_id
    // and board_id NULL) are reparented to the created issue by
    // `issues.create({ draftId })` and die with the draft (cascade + S3
    // reclaim in `issueDrafts.delete`). The attachments SHAPE excludes them
    // (`issue_id IS NOT NULL`), so the natives' non-optional issueId holds.
    issueId: uuid(`issue_id`).references(() => issues.id, {
      onDelete: `cascade`,
    }),
    // Denormalized from issue→board (populate_issue_child_board_id) so the
    // trash fan-out can target a board's child rows. Attachment byte reads
    // are member-only too (EXP-180). NULL on draft rows: their board is the
    // draft's, and the trash purge collects them through issue_drafts.
    boardId: uuid(`board_id`).references(() => boards.id, {
      onDelete: `cascade`,
    }),
    // EXP-878: the owning draft. Cascade: deleting the draft removes the row
    // (the router reclaims the blob after commit).
    draftId: uuid(`draft_id`).references(() => issueDrafts.id, {
      onDelete: `cascade`,
    }),
    // EXP-1291: the conversation message the file rides (`sessions.send
    // ({attachmentIds})` claims eager uploads; replaces session_attachments).
    // NULL on all three owners = an upload not yet sent (the orphan sweep
    // reclaims it after the grace window). Never synced (the attachments
    // shape keeps `issue_id IS NOT NULL`); bytes via /api/attachments/{id}.
    messageId: uuid(`message_id`).references((): AnyPgColumn => messages.id, {
      onDelete: `cascade`,
    }),
    // Mirrors of the parent board's deleted_at / archived_at
    // (trigger-maintained, REV2-5 + EXP-500) — the shape's static trash and
    // archive predicates. Server-only, shape-excluded.
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    commentId: uuid(`comment_id`).references(() => comments.id, {
      onDelete: `set null`,
    }),
    // NULLABLE: widget screenshot attachments have no user uploader (the
    // synthetic per-widget bot user was removed). `set null` on user delete
    // (REV2-36): an attachment is embedded in an issue description or comment
    // as `![alt](/api/attachments/{id})`, and those bodies survive the
    // uploader's account deletion (issues.creator_id is `set null`, and any
    // member may embed an image into a teammate's issue) — cascading the
    // attachment away left permanently broken images in content the deletion
    // was never supposed to touch. Blobs are only reclaimed for the teams the
    // deletion itself destroys (lib/account-deletion.ts).
    uploaderId: text(`uploader_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    filename: varchar({ length: 500 }).notNull(),
    contentType: varchar(`content_type`, { length: 255 }).notNull(),
    sizeBytes: bigint(`size_bytes`, { mode: `number` }).notNull(),
    storageKey: text(`storage_key`).notNull(),
    url: text().notNull(),
    // Intrinsic pixel dimensions, probed at upload time. Nullable so legacy
    // rows and attachments whose format we can't measure stay valid; clients
    // use them to reserve aspect-ratio space and avoid layout shift.
    width: integer(),
    height: integer(),
    // EXP-824: inline video/audio. `duration_ms` is probed from the MP4/MOV
    // header at upload (or supplied by the normalising client for containers
    // the server does not parse); the poster frame is a SECOND blob keyed by
    // `poster_storage_key` on the same row (never its own attachments row, so
    // it can't be swept as an unreferenced image and dies with its video —
    // every reclaim path collects both keys via `attachmentStorageKeys`).
    // Served by the byte route as `/api/attachments/{id}?poster=1`.
    durationMs: integer(`duration_ms`),
    posterStorageKey: text(`poster_storage_key`),
    // The poster blob's byte size, so the team storage budget counts it
    // (`getTeamUsage` sums it beside `size_bytes`). SERVER-ONLY: not in the
    // attachments shape allowlist. NULL when the row has no poster.
    posterSizeBytes: integer(`poster_size_bytes`),
    // EXP-1247: uploaded through a FILE/paperclip path, so the row lists in
    // Files even when its type would inline (an image picked as a file); the
    // unreferenced-image sweep exempts it. Image button / paste / drop = false.
    asFile: boolean(`as_file`).notNull().default(false),
    ...timestamps,
  },
  (table) => [
    check(
      `attachments_owner_check`,
      sql`num_nonnulls(issue_id, draft_id, message_id) <= 1`
    ),
    index(`idx_attachments_issue`).on(table.issueId),
    index(`idx_attachments_team`).on(table.teamId),
    index(`idx_attachments_board`).on(table.boardId),
    // EXP-878: the draft FK cascade + the reparent UPDATE. Partial — most
    // attachments belong to issues.
    index(`idx_attachments_message`)
      .on(table.messageId)
      .where(sql`message_id IS NOT NULL`),
    index(`idx_attachments_draft`)
      .on(table.draftId)
      .where(sql`draft_id IS NOT NULL`),
    // The comment_id SET NULL RI trigger fires on every comment delete.
    // Partial — most attachments aren't comment-embedded.
    index(`idx_attachments_comment`)
      .on(table.commentId)
      .where(sql`comment_id IS NOT NULL`),
  ]
)


// One row per (token, user): several accounts signed in on one device each
// keep their own registration of the shared FCM device token, so pushes reach
// every account instead of only the most recently registered one. Dead-token
// cleanup deletes by token value across users (FCM invalidates per device).
export const fcmTokens = pgTable(
  `fcm_tokens`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    token: text().notNull(),
    platform: varchar({ length: 20 }).notNull(),
    ...timestamps,
  },
  (table) => [
    unique().on(table.token, table.userId),
    // Per-recipient push lookups (fcm.ts, push-tokens router) filter by user;
    // the (token, user_id) unique can't serve them.
    index(`idx_fcm_tokens_user`).on(table.userId),
  ]
)

// EXP-481: a device's SERVER-AUTHORITATIVE per-agent launch defaults. The
// devices row is the source of truth; the device's local settings.json is a
// converging cache (it applies server changes on heartbeat/nudge and pushes
// local edits up via devices.setLaunchDefaults with a compare-and-set on
// launch_defaults_updated_at). Inner keys are camelCase verbatim on every
// client — column mappers only rename top-level columns. Vocabulary
// validation (agent ids, model/effort sets) lives in the web app; this schema
// is structural bounds only, byte-parity with the relay online-frame shape.
export interface DeviceAgentLaunchDefaults {
  model?: string
  effort?: string
  /** EXP-981: claude only; absent = contract deviceAgentDefaults.subagentModel
   * (never the CLI's own default: every model is explicit). */
  subagentModel?: string
  /** The dispatcher root's model (threads + team agents); absent = contract
   * deviceAgentDefaults.threadModel / codexThreadModel. */
  threadModel?: string
  ultracode?: boolean
  planMode?: boolean
  /** EXP-1082 §6: rotate to another logged-in account of this agent when
   * the active one hits its usage wall (claude only in practice; the
   * schema just allows the key). Absent = off. Seam only: no UI yet. */
  autoRotateAccounts?: boolean
}
export interface DeviceLaunchDefaults {
  /** EXP-1158: the LAST USED agent, written by the DEVICE when a person
   * starts a run on it (clients never send it; a save without it keeps the
   * stored one). The last used ACCOUNT is that agent's `active` profile in
   * `agent_accounts`. */
  defaultAgent?: string
  /** EXP-1196: runs on this device get a local computer-use MCP server
   * (screen, click, type). Device-level, absent = off; a save without it
   * keeps the stored one. */
  computerUse?: boolean | null
  /** EXP-1236: the model alias the run's screen-driving subagents run on
   * (contract `computerUseModel`); absent = contract
   * `deviceComputerUseDefaults.model`. Same carry-forward as `computerUse`. */
  computerUseModel?: string | null
  /** NEW-59 / #FEED-161: the WORK SCREEN, the stable `id` of the display
   * (in the device's own `doctor.displays` list) agents drive with computer
   * use; absent / null = Auto (every display without a fullscreen app). A
   * save without it keeps the stored one, an explicit null clears it to Auto
   * (spec: domain-contract `fixtures/work-screen.json`). */
  computerUseDisplayId?: string | null
  agents?: Record<string, DeviceAgentLaunchDefaults>
}
// Every field is `.nullish()`, not `.optional()`: an explicit `null` clears
// a field (EXP-495). `clampLaunchDefaults` strips the nulls; stored copies
// stay null-free.
// Plain `z.object` (strip mode) on purpose — never `.strict()`: an unknown
// key from a newer or older client is dropped, not a reason to 400 the
// register.
export const deviceLaunchDefaultsSchema = z.object({
  defaultAgent: z.string().min(1).max(32).nullish(),
  // EXP-1196: see DeviceLaunchDefaults.computerUse.
  computerUse: z.boolean().nullish(),
  // EXP-1236: see DeviceLaunchDefaults.computerUseModel.
  computerUseModel: z.string().max(32).nullish(),
  // NEW-59 / #FEED-161: see DeviceLaunchDefaults.computerUseDisplayId.
  computerUseDisplayId: z.string().min(1).max(128).nullish(),
  agents: z
    .record(
      z.string().min(1).max(32),
      z.object({
        model: z.string().max(64).nullish(),
        effort: z.string().max(64).nullish(),
        // EXP-981: claude only — the model its subagents run on.
        subagentModel: z.string().max(64).nullish(),
        // The dispatcher root's model (threads + team agents).
        threadModel: z.string().max(64).nullish(),
        ultracode: z.boolean().nullish(),
        planMode: z.boolean().nullish(),
        // EXP-1082 §6: see DeviceAgentLaunchDefaults.autoRotateAccounts.
        autoRotateAccounts: z.boolean().nullish(),
      })
    )
    .refine((agents) => Object.keys(agents).length <= 16)
    .nullish(),
})

// EXP-484: the machine's READ-ONLY per-agent auth + usage status, collected
// locally by the device (it never holds, copies or refreshes a credential)
// and shipped on register/heartbeat. Keyed by contract `codingAgent` id;
// inner keys are camelCase verbatim on every client (column mappers only
// rename top-level columns). `checkedAt`/`fetchedAt`/`resetsAt` are ISO
// strings, `percent` is 0-100. Presentation (window selection, severity,
// freshness, captions) is hand-mirrored ×4 — web `lib/agent-usage.ts`.
export interface DeviceAgentAccount {
  signedIn: boolean
  email?: string
  plan?: string
  checkedAt?: string
  /** EXP-849: how USABLE the login is, as the device's usage probe saw it
   * (`auth status` is identity only, never health) — a probe that came back
   * Unauthorized is `needs_relogin`, a successful one `ok`, a profile that is
   * not signed in `signed_out`, and one signed in but never probed
   * `unknown`. Absent on pre-EXP-849 devices: derive it from `signedIn`
   * (true → `ok`, false → `signed_out`). */
  health?: DeviceAgentHealth
  /** EXP-792 (EXP-747 B5): every profile on the device, ≤`MAX_AGENT_PROFILES`;
   * absent on pre-profile clients. The top-level fields stay the ACTIVE
   * profile. */
  profiles?: DeviceAgentProfileEntry[]
  /** The agent CLI's installed version as the device's doctor last read it
   * (`2.1.281`). A property of the INSTALL, never of a login, so it rides
   * the top-level row only. The device settings' Update section shows it
   * beside the per-agent "Update" control. */
  version?: string
  /** Present iff the agent CLI's AMBIENT login (~/.claude, ~/.codex) is
   * signed in. Runs never use it; the doctor offers to Import (move) it into
   * a profile. */
  importable?: { email?: string; plan?: string }
}
export interface DeviceAgentProfileEntry {
  id: string
  signedIn: boolean
  email?: string
  plan?: string
  active?: boolean
  checkedAt?: string
  /** When a sign-in or import last COMMITTED into this profile (ISO). An open
   * sign-in dialog lands when this moves. */
  lastLoginAt?: string
  /** EXP-849: this profile's own health, same vocabulary + same fallback as
   * the account's. */
  health?: DeviceAgentHealth
  /** EXP-849: this login's IDENTITY ships but its usage numbers are not
   * collected — it sits past the device's usage-probe cap
   * (`coding::agent_usage::MAX_USAGE_PROFILES`), so one heartbeat cannot fan
   * out into a probe per login. Absent = monitored. */
  unmonitored?: boolean
  usage?: DeviceAgentUsage
}

/** EXP-849: the four health values, byte-identical with the desktop's
 * `coding::agent_accounts::AgentHealth` wire strings. The server clamp keeps
 * ONLY these (an unknown value is dropped, never a 400 — a newer device must
 * lose a field, not its whole heartbeat). */
export const deviceAgentHealthValues = [
  `ok`,
  `needs_relogin`,
  `signed_out`,
  `unknown`,
] as const
export type DeviceAgentHealth = (typeof deviceAgentHealthValues)[number]
export type DeviceAgentAccounts = Record<string, DeviceAgentAccount>

export interface DeviceUsageWindow {
  key: string
  label: string
  percent: number
  resetsAt?: string | null
}
export interface DeviceAgentUsage {
  fetchedAt?: string
  stale?: boolean
  windows: DeviceUsageWindow[]
}
export type DeviceAgentUsageMap = Record<string, DeviceAgentUsage>

// Every field is `.nullish()` for the same reason the launch defaults are
// (EXP-495): a client serializing an absent field as explicit `null` must
// degrade that field, never 400 the whole register and leave the machine
// invisible. Structural bounds only — `clampAgentAccounts`/`clampAgentUsage`
// (lib/trpc/devices.ts) own the vocabulary and the stored copies stay
// null-free.
// EXP-792 (EXP-747 B5): N accounts per agent ride the SAME jsonb value —
// `signedIn`/`email`/`plan`/`checkedAt` stay the ACTIVE profile (old clients
// see exactly the pre-profile payload), `profiles` lists every profile the
// device holds (≤`MAX_AGENT_PROFILES`, exactly its profile dirs; a login is
// named by its email, never a label). No new column: an unknown devices
// column bricks older native sync.
export const deviceAgentProfileSchema = z.object({
  id: z.string().max(64),
  signedIn: z.boolean().nullish(),
  email: z.string().max(320).nullish(),
  plan: z.string().max(64).nullish(),
  active: z.boolean().nullish(),
  checkedAt: z.string().max(64).nullish(),
  // When a sign-in or import last COMMITTED into this profile (ISO).
  lastLoginAt: z.string().max(64).nullish(),
  // EXP-849: a STRING, not a z.enum — a device reporting a health value this
  // build has no name for must lose the field in the clamp, not 400 the
  // whole register (`clampAgentAccounts` owns the vocabulary).
  health: z.string().max(32).nullish(),
  // EXP-849: the device collects no usage for this login (it sits past its
  // own probe cap) — the row is identity-only, and the clients say so rather
  // than rendering an empty bar as "0%".
  unmonitored: z.boolean().nullish(),
  usage: z
    .object({
      fetchedAt: z.string().max(64).nullish(),
      stale: z.boolean().nullish(),
      windows: z
        .array(
          z
            .object({
              key: z.string().max(64).nullish(),
              label: z.string().max(64).nullish(),
              percent: z.number().nullish(),
              resetsAt: z.string().max(64).nullish(),
            })
            .nullish()
        )
        .nullish(),
    })
    .nullish(),
})
/** EXP-849: how many profiles per agent one device may report. A machine
 * reports EVERY login it holds, not just the ones it probes (the ones past
 * the desktop's usage-probe cap `coding::agent_usage::MAX_USAGE_PROFILES`
 * ride along flagged `unmonitored`), so this ceiling has to sit well above
 * that cap — silently dropping profiles here would hide logins the account
 * pickers are supposed to offer. */
export const MAX_AGENT_PROFILES = 24

export const deviceAgentAccountsSchema = z.record(
  z.string(),
  z
    .object({
      signedIn: z.boolean().nullish(),
      email: z.string().max(320).nullish(),
      plan: z.string().max(64).nullish(),
      checkedAt: z.string().max(64).nullish(),
      // EXP-849: see `deviceAgentProfileSchema.health`.
      health: z.string().max(32).nullish(),
      profiles: z.array(deviceAgentProfileSchema.nullish()).nullish(),
      // The install's CLI version (`DeviceAgentAccount.version`).
      version: z.string().max(64).nullish(),
      // `DeviceAgentAccount.importable`.
      importable: z
        .object({
          email: z.string().max(320).nullish(),
          plan: z.string().max(64).nullish(),
        })
        .nullish(),
    })
    .nullish()
)

export const deviceAgentUsageSchema = z.record(
  z.string(),
  z
    .object({
      fetchedAt: z.string().max(64).nullish(),
      stale: z.boolean().nullish(),
      windows: z
        .array(
          z
            .object({
              key: z.string().max(64).nullish(),
              label: z.string().max(64).nullish(),
              percent: z.number().nullish(),
              resetsAt: z.string().max(64).nullish(),
            })
            .nullish()
        )
        .nullish(),
    })
    .nullish()
)

// EXP-1196/1218/1219: the device's readiness report (`coding::device_doctor`),
// shipped on register + heartbeat and rendered by every client as ONE block
// (spec: domain-contract `fixtures/device-doctor.json`). The device writes
// `detail`; clients never compose it. The key/group/state/action sets below
// are the fixture's closed vocabulary, but the schema PARSES LENIENTLY: an
// unknown string is kept (length-capped) so a newer device never 400s its
// register; clients skip what they cannot name.
export const deviceDoctorKeys = [
  `git`,
  `claude`,
  `codex`,
  `computer_use`,
  `screen_recording`,
  `accessibility`,
  // NEW-78: the voice device's microphone (IDE only, Push to talk on).
  `microphone`,
  // NEW-55: whether the voice device can speak (IDE only; S-3: whenever it
  // is the member's voice device, speak-back is always on).
  `speech`,
  // S-3: the wake word's state on the voice device (IDE only, wakeWord on):
  // ok while listening, else off; `detail` = its `wakeDetail` text, which
  // every other client's Wake word row shows verbatim (voice.json).
  `wake`,
  // EXP-1375: free disk (the kept worktree rows are `worktree-<n>`, labelled).
  `disk`,
  // NEW-57: the voice device's desktop audio (IDE only, desktopAudio not off).
  `desktop_audio`,
] as const
export const deviceDoctorGroups = [
  `required`,
  `agents`,
  `computer_use`,
  `voice`,
  `storage`,
] as const
export const deviceDoctorStates = [
  `ok`,
  `action`,
  `missing`,
  `off`,
  `error`,
] as const
export const deviceDoctorActions = [
  `install`,
  `update`,
  `sign_in`,
  `import`,
  `grant`,
] as const
export type DeviceDoctorKey = (typeof deviceDoctorKeys)[number]
export type DeviceDoctorGroup = (typeof deviceDoctorGroups)[number]
export type DeviceDoctorState = (typeof deviceDoctorStates)[number]
export type DeviceDoctorAction = (typeof deviceDoctorActions)[number]
export interface DeviceDoctorItem {
  key: DeviceDoctorKey | (string & {})
  group: DeviceDoctorGroup | (string & {})
  parent?: string
  /** EXP-1375: the row's own label (a kept worktree's directory name). */
  label?: string
  state: DeviceDoctorState | (string & {})
  detail?: string
  action?: DeviceDoctorAction | (string & {})
  /** The agent's AMBIENT login email: renders an `import` pill before the
   * action pill. */
  import?: string
}
/** NEW-59: one display as the device reports it (the OS's order, pixels). */
export interface DeviceDisplay {
  /** #FEED-161: stable across reconnects and reorders (1..128 chars), the
   * value `launch_defaults.computerUseDisplayId` stores. */
  id: string
  /** 1-based position in the OS's order, for the label only. */
  number: number
  width: number
  height: number
  primary: boolean
}
export interface DeviceDoctor {
  checkedAt: string
  items: DeviceDoctorItem[]
  /** NEW-59: the device's displays (the Work screen picker's options);
   * absent = not reported (an older build, a daemon without a display). */
  displays?: DeviceDisplay[]
  /** NEW-57: what desktop audio the voice device can capture and the apps
   * playing audio right now (meeting.json); absent = `none` and `[]`. */
  desktopAudio?: DeviceDesktopAudio
}
/** NEW-57: `capability` stays a lenient string (clients read an unknown one
 * as `none`). */
export interface DeviceDesktopAudio {
  capability: `none` | `system` | `per_app` | (string & {})
  apps: { id: string; name: string }[]
}
export const MAX_DESKTOP_AUDIO_APPS = 32
// EXP-1375: room for up to 12 kept worktree rows beside the checks.
export const MAX_DEVICE_DOCTOR_ITEMS = 32
export const deviceDoctorItemSchema = z.object({
  key: z.string().min(1).max(32),
  group: z.string().min(1).max(32),
  parent: z.string().max(32).nullish(),
  label: z
    .string()
    .nullish()
    .transform((v) => (v == null ? v : v.slice(0, 120))),
  state: z.string().min(1).max(32),
  // Truncated, never refused: a long device-written line must not 400.
  detail: z
    .string()
    .nullish()
    .transform((v) => (v == null ? v : v.slice(0, 120))),
  action: z.string().max(32).nullish(),
  import: z.string().max(320).nullish(),
})
export const deviceDoctorSchema = z.object({
  checkedAt: z.string().max(64),
  // Extra items are dropped, not refused.
  items: z
    .array(deviceDoctorItemSchema)
    .max(64)
    .transform((items) => items.slice(0, MAX_DEVICE_DOCTOR_ITEMS)),
  // NEW-59: see DeviceDoctor.displays. #FEED-161: lenient, an entry without
  // a usable `id` is dropped, never a reason to 400 the heartbeat.
  displays: z
    .preprocess(
      (value) =>
        Array.isArray(value)
          ? value.filter(
              (entry) =>
                typeof entry === `object` &&
                entry !== null &&
                typeof (entry as { id?: unknown }).id === `string` &&
                (entry as { id: string }).id.length >= 1 &&
                (entry as { id: string }).id.length <= 128
            )
          : value,
      z.array(
        z.object({
          id: z.string().min(1).max(128),
          number: z.number().int().min(1).max(16),
        width: z.number().int().min(1).max(100000),
        height: z.number().int().min(1).max(100000),
        primary: z.boolean(),
        })
      )
      .max(16)
    )
    .nullish(),
  // NEW-57: see DeviceDoctor.desktopAudio. Lenient: extra apps are dropped,
  // long names truncated, never refused.
  desktopAudio: z
    .object({
      capability: z.string().min(1).max(32),
      apps: z
        .array(
          z.object({
            id: z.string().min(1).max(256),
            name: z
              .string()
              .max(512)
              .transform((v) => v.slice(0, 64)),
          })
        )
        .max(256)
        .transform((apps) => apps.slice(0, MAX_DESKTOP_AUDIO_APPS)),
    })
    .nullish(),
})

// EXP-403 registered devices — since EXP-481 an Electric shape (own rows plus
// team-shared server rows; EXP-639 retired the devices router `list`).
// One row per (user, deviceId): desktops and headless `exponential` daemon
// servers register on control-channel/daemon start and heartbeat
// `last_seen_at` (~30s; online = freshness within the contract window).
// `kind` is a documented varchar (`desktop` | `server`), no contract enum.
export const devices = pgTable(
  `devices`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // The steer deviceId (settings.json `deviceId`, ≤128 chars on the relay).
    deviceId: varchar(`device_id`, { length: 128 }).notNull(),
    label: varchar({ length: 255 }).notNull(),
    kind: varchar({ length: 32 }).notNull(),
    platform: varchar({ length: 64 }),
    // EXP-924: the owner-picked display icon (deviceIconValues in domain.ts /
    // the contract's deviceIcon). NULL = the kind default every client
    // derives (`monitor` for a desktop, `server` for a daemon), so register
    // never writes it and new machines keep those two glyphs.
    icon: varchar({ length: 32 }),
    // The client's marketing version (`0.8.52`), refreshed on every register.
    version: varchar({ length: 32 }),
    // Web "Update" button (EXP-403): set by devices.requestUpdate, surfaced
    // to the daemon via the heartbeat response, cleared by the next register
    // (the daemon re-registers after acting on the request).
    updateRequestedAt: timestamp(`update_requested_at`, { withTimezone: true }),
    // EXP-411: live coding sessions the daemon supervises, refreshed on every
    // heartbeat (off-cadence whenever the count changes). Heartbeat-owned —
    // register never touches it. A pending update request parks until this
    // reaches 0, and the machine rows say "Update queued" instead of spinning
    // forever. Stays 0 for desktops and pre-EXP-411 daemons omitting the field.
    activeSessions: integer(`active_sessions`).notNull().default(0),
    // FEED-145: the last heartbeat's `liveSessionIds` (NEW-5). Server-only.
    // A session listed here on an online device has a live process whose
    // own stall watchdog (FEED-25) guards its turn, so the server watchdog
    // never flags it `stuck` for a long, report-free turn. NULL = the
    // device never claimed (an older client): the 30-minute rule applies.
    liveSessionIds: text(`live_session_ids`).array(),
    agents: jsonb()
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    caps: jsonb()
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    lastSeenAt: timestamp(`last_seen_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
    // EXP-432/FEED-33: the teams this device is shared with (server-kind
    // only, router-enforced; sorted + deduped by `devices.setShared`).
    // Teammates of any listed team see the device in that team's device
    // list and may remote-start sessions on it; the resulting rows are
    // requester-owned with host_user_id = this row's owner. `{}` = private
    // (the default). An array carries no FK: trigger #16
    // (unshare_deleted_team) strips a deleted team's id.
    sharedTeamIds: uuid(`shared_team_ids`)
      .array()
      .notNull()
      .default(sql`'{}'::uuid[]`),
    // EXP-622: the OWNER's default machine — the one every device picker
    // prefills when several are candidates. At most one true row per user
    // (`devices.setDefault` clears the others in the same transaction).
    // Meaningless to a teammate reading a SHARED row: it is the owner's
    // preference, so clients honour the flag only on their own rows.
    isDefault: boolean(`is_default`).notNull().default(false),
    // EXP-481: agents installed but signed out — persisted mirror of the
    // EXP-409 live advertisement, refreshed on register, so offline rows can
    // still explain themselves.
    unauthedAgents: jsonb(`unauthed_agents`)
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    // EXP-749: the subset of `agents` the machine's ACP engine can drive
    // (the doctor's per-agent `acp` verdict), refreshed on register. NULL =
    // the build never reported (pre-EXP-749) and clients assume every
    // runnable agent, exactly the old behaviour; EXP-773: an agent listed
    // in `agents` but not here cannot start a session there at all, and
    // remote pickers say so instead of letting the launch fail.
    acpAgents: jsonb(`acp_agents`).$type<string[]>(),
    // EXP-481: server-authoritative launch defaults (see the doc block on
    // DeviceLaunchDefaults above). NULL = the device never reported and no
    // client ever edited — clients seed static contract defaults.
    launchDefaults: jsonb(`launch_defaults`).$type<DeviceLaunchDefaults>(),
    // The compare-and-set stamp for device pushes: a device echoes the stamp
    // it last converged to and the server refuses stale writes (server wins
    // offline-concurrent races). Equality compare, never `>` — no clock-skew
    // semantics.
    launchDefaultsUpdatedAt: timestamp(`launch_defaults_updated_at`, {
      withTimezone: true,
    }),
    // EXP-484: per-agent sign-in status (see DeviceAgentAccount above) as the
    // machine last probed it. NULL = never reported — clients
    // render no Agents section rather than claiming "signed out".
    agentAccounts: jsonb(`agent_accounts`).$type<DeviceAgentAccounts>(),
    // EXP-484: per-agent usage windows as the machine last fetched them,
    // plus the stamp of that write. The stamp moves every few minutes and is
    // NEVER a convergence trigger: the desktop watches
    // `launch_defaults_updated_at` only, or every usage refresh would nudge
    // the fleet.
    agentUsage: jsonb(`agent_usage`).$type<DeviceAgentUsageMap>(),
    agentUsageAt: timestamp(`agent_usage_at`, { withTimezone: true }),
    // EXP-1196: the device's readiness report (DeviceDoctor above), written
    // on register + heartbeat. NULL = not reported yet: clients render
    // no readiness block.
    doctor: jsonb().$type<DeviceDoctor>(),
    // FEED-154: the machine's ed25519 public key (64 hex chars) the LAST
    // register PROVED it holds (a fresh signature over user + device id).
    // Server-only, outside the shape allowlist. A share needs it to equal
    // the machine's pin (`device_keys`): an account that only knows a box's
    // id cannot share it. NULL = registered without a proof (a desktop).
    publicKey: varchar(`public_key`, { length: 64 }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.userId, table.deviceId),
    index(`idx_devices_user`).on(table.userId),
    // Serves the per-team shared listing (`&&` / `@>` on the array).
    index(`idx_devices_shared_teams`).using(`gin`, table.sharedTeamIds),
  ]
)

// FEED-154: the key PINNED for a machine id, trust-on-first-use: the first
// proven register of the id pins its key, and only the pinning account may
// rotate it (a fresh data dir). It outlives the devices rows on purpose: the
// box owner's Remove must not hand the pin to a teammate's squat of the id.
// An instance admin clears a wrong pin (`admin.clearDeviceKey`). Server-only.
export const deviceKeys = pgTable(`device_keys`, {
  deviceId: varchar(`device_id`, { length: 128 }).primaryKey(),
  publicKey: varchar(`public_key`, { length: 64 }).notNull(),
  userId: text(`user_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  ...timestamps,
})

// FEED-159: the account that registered a machine id FIRST, the owner signal
// for pinning (`pinDeviceKeyInTx`). The id shows in team-visible sessions
// only after the real daemon registered it, so the first account to
// register it is the box's. Written insert-or-ignore on every register and
// NEVER deleted by Remove: the eldest `devices` row stopped being that signal
// once the owner's Remove + relaunch made its row younger than a squat's. An
// instance admin moves it (`admin.clearDeviceKey({pinTo})`). Server-only.
export const deviceFirstSeen = pgTable(`device_first_seen`, {
  deviceId: varchar(`device_id`, { length: 128 }).primaryKey(),
  userId: text(`user_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  firstSeenAt: timestamp(`first_seen_at`, { withTimezone: true }).notNull().defaultNow(),
})

// EXP-1417: a machine its owner REMOVED. A running daemon or desktop app
// re-registered the row on its next heartbeat miss or re-dial, so Remove
// never stuck; while this row stands only a fresh launch of that build
// (`register({launch: true})`) brings the machine back. Server-only.
export const deviceRemovals = pgTable(
  `device_removals`,
  {
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    deviceId: varchar(`device_id`, { length: 128 }).notNull(),
    removedAt: timestamp(`removed_at`, { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [primaryKey({ columns: [table.userId, table.deviceId] })]
)

// EXP-481: per-device worktree inventory, reported by the device
// (devices.reportWorktrees, full current set diff-upserted server-side) —
// powers resume offers, listing and prune even while the device is offline.
// Synced shape #18. `user_id` + `shared_team_ids` are trigger-maintained
// mirrors of the owning devices row (populate_device_worktree_owner +
// propagate_device_shared_team) so the shape's where clause stays
// single-table and its identity rotates ONLY on team-membership changes
// (REV2-5 stance); both stay OUT of the shape's column allowlist.
export const deviceWorktrees = pgTable(
  `device_worktrees`,
  {
    id: uuidPk(),
    deviceRowId: uuid(`device_row_id`)
      .notNull()
      .references(() => devices.id, { onDelete: `cascade` }),
    // Trigger-derived mirrors — never client input.
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    sharedTeamIds: uuid(`shared_team_ids`)
      .array()
      .notNull()
      .default(sql`'{}'::uuid[]`),
    repoFullName: varchar(`repo_full_name`, { length: 255 }).notNull(),
    branch: varchar({ length: 255 }).notNull(),
    // `exp/<IDENTIFIER>` linkage as the DEVICE parsed it (the branch prefix
    // is a device-local setting); clients join against their own synced
    // issues — no server enrichment.
    issueIdentifier: varchar(`issue_identifier`, { length: 64 }),
    // Agents recorded in the worktree's .exp-agents resume marker; NULL =
    // pre-marker worktree (any agent may resume).
    agents: jsonb().$type<string[]>(),
    // Documented varchar (`clean` | `untracked` | `tracked` | `unknown`), no
    // contract enum — a future device vocabulary must not break old servers.
    dirty: varchar({ length: 32 }).notNull().default(`unknown`),
    // A live local session currently holds this worktree's branch.
    busy: boolean().notNull().default(false),
    reportedAt: timestamp(`reported_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
    ...timestamps,
  },
  (table) => [
    unique().on(table.deviceRowId, table.repoFullName, table.branch),
    index(`idx_device_worktrees_user`).on(table.userId),
    index(`idx_device_worktrees_shared_teams`).using(
      `gin`,
      table.sharedTeamIds
    ),
  ]
)

// EXP-481: owner→device work queue (SERVER-ONLY, never synced — a bilateral
// concern, not team product data). Created by the owner via
// devices.createCommand, delivered on the device's heartbeat (plus a relay
// check_in nudge for immediacy), completed via devices.completeCommand.
// Rows stay `pending` until completed — redelivery on a missed cycle is free
// idempotency. `kind` is a documented varchar (EXP-1060 retired
// `worktree_remove`/`worktree_prune`): `agent_login` (EXP-484, payload {agent, switch: "true"|"false", profileId?,
// newProfileLabel?}: the device runs the agent CLI's own login flow inside
// the named account profile (EXP-827: `profileId` = an existing profile,
// `newProfileLabel` = create one first, neither = the ambient login) and
// completes the command EARLY, as soon as the sign-in URL is on
// screen, with the JSON progress in `result` ({agent, phase, url?, code?,
// message?, profileId}, `profileId` = the id it signed into, `system` when
// none was named)) |
// `agent_login_code` (payload {agent, code} — the typed device code for a
// pending login) |
// `agent_usage_refresh` (EXP-747 C4, payload {agent, profileId} — force a
// usage collection past the shared TTL, never past the rate-limit floor) |
// `agent_profile_remove` (EXP-862, payload {agent, profileId} — delete THIS
// machine's copy of a login: the profile dir and its index row, never the
// account; EXP-1137: `system` = sign the ambient login out there and hide
// its row until it signs in again, cap `account-sign-out`) |
// `agent_profile_sign_out` (EXP-1137, payload {agent, profileId} — sign ONE
// login out on the machine and keep its row: claude's own `auth logout` in
// that profile's config dir, codex's credential file deleted, never `codex
// logout`; cap `account-sign-out`) |
// `update_now` (FEED-36, payload {} — end every live session on the machine
// and restart on the queued self-update; cap-gated on `update-now`) |
// `agent_update` (payload {agent} — run that agent CLI's own self-updater on
// the machine, `claude update`/`codex update`, and re-probe so the heartbeat's
// `agent_accounts.<agent>.version` moves; the result names the version move;
// idempotent).
export const deviceCommands = pgTable(
  `device_commands`,
  {
    id: uuidPk(),
    deviceRowId: uuid(`device_row_id`)
      .notNull()
      .references(() => devices.id, { onDelete: `cascade` }),
    // The device OWNER (the only allowed creator) — denormalized so
    // completeCommand/getCommand authorize on one indexed predicate.
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    kind: varchar({ length: 32 }).notNull(),
    payload: jsonb()
      .$type<Record<string, string>>()
      .notNull()
      .default(sql`'{}'::jsonb`),
    // pending → done | failed.
    status: varchar({ length: 16 }).notNull().default(`pending`),
    // Device-reported result message (aggregated prune summary, refusal
    // reason, ...).
    result: text(),
    completedAt: timestamp(`completed_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    // Serves the heartbeat pickup; partial — terminal rows are history.
    index(`idx_device_commands_pending`)
      .on(table.deviceRowId)
      .where(sql`status = 'pending'`),
  ]
)

// EXP-1111: one-time CLI install tokens (SERVER-ONLY, never synced). The Add
// device dialog mints one (`devices.createInstallToken`) and bakes it into
// the install one-liner as EXP_INSTALL_TOKEN; the freshly installed daemon
// trades it ONCE at the anonymous `POST /api/cli/install-token/redeem` for a
// regular Better Auth session token — the same credential the device-code
// login yields. Only the sha256 hex of the token is stored; `used_at` is the
// atomic single-use claim; rows are short-lived (15-minute TTL).
export const cliInstallTokens = pgTable(
  `cli_install_tokens`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    tokenHash: text(`token_hash`).notNull().unique(),
    expiresAt: timestamp(`expires_at`, { withTimezone: true }).notNull(),
    usedAt: timestamp(`used_at`, { withTimezone: true }),
    createdAt: timestamp(`created_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [index(`idx_cli_install_tokens_user`).on(table.userId)]
)

export const notifications = pgTable(
  `notifications`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    issueId: uuid(`issue_id`).references(() => issues.id, {
      onDelete: `cascade`,
    }),
    // Trigger-denormalized from the issue (0001_triggers.sql §7) so the
    // trash fan-out can target a board's notification rows. Server-only
    // scoping — excluded from the shape via its columns allowlist, like
    // emailed_at. Nullable like issue_id: an issue-less notification carries
    // no board identity.
    boardId: uuid(`board_id`).references(() => boards.id, {
      onDelete: `cascade`,
    }),
    // Mirrors of the parent board's deleted_at / archived_at
    // (trigger-maintained, REV2-5 + EXP-500) — the shape's static trash and
    // archive predicates; both stay NULL on issue-less rows, which therefore
    // always sync. Server-only, shape-excluded.
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    // App-written team pointer for ISSUE-LESS rows (`agent_message`,
    // `session_blocked`): with no issue to resolve a team from, clients need
    // this to route the row. Synced (in the shape allowlist), unlike
    // board_id. Stays NULL on issue-anchored rows — their team comes from
    // the issue.
    teamId: uuid(`team_id`).references(() => teams.id, {
      onDelete: `cascade`,
    }),
    // EXP-980: the coding run a `session_blocked` row is about. Synced (in
    // the shape allowlist) so the inbox row routes to the run ×4; SET NULL
    // keeps the delivered row when the run's row is pruned.
    sessionId: uuid(`session_id`).references(() => sessions.id, {
      onDelete: `set null`,
    }),
    type: notificationTypeEnum().notNull(),
    title: varchar({ length: 500 }).notNull(),
    body: text(),
    readAt: timestamp(`read_at`, { withTimezone: true }),
    pushedAt: timestamp(`pushed_at`, { withTimezone: true }),
    // Stamped once the hourly email digest has handled this row (bundled into
    // a digest email OR claimed as email-opted-out). NULL = the digest hasn't
    // considered it yet. Server-only delivery bookkeeping — excluded from the
    // notifications shape via its columns allowlist.
    emailedAt: timestamp(`emailed_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_notifications_user_unread`).on(table.userId, table.readAt),
    index(`idx_notifications_board`).on(table.boardId),
    // notifications is the largest table (one row per recipient per event) and
    // its issue_id FK cascade fires per deleted issue — without this every
    // issue delete (and issues.move's board_id rewrite) seq-scans the table.
    index(`idx_notifications_issue`).on(table.issueId),
    // Bounds the deliver()/deliverToTeam 30s dedupe NOT EXISTS (recipient +
    // recency) — without it the team-wide fan-out walks each recipient's
    // full notification history per event.
    index(`idx_notifications_user_created`).on(
      table.userId,
      table.createdAt.desc()
    ),
    // The hourly digest sweep's scan: unread, never-emailed rows by age.
    index(`idx_notifications_digest_pending`)
      .on(table.createdAt)
      .where(sql`read_at IS NULL AND emailed_at IS NULL`),
    // The admin Performance tab's 90-day totals/by-day windows filter on
    // created_at alone (EXP-553) — without this each 60s poll seq-scans the
    // largest table.
    index(`idx_notifications_created`).on(table.createdAt),
    // EXP-980: the session_id SET NULL fires per deleted run; partial, since
    // only `session_blocked` rows carry one.
    index(`idx_notifications_session`)
      .on(table.sessionId)
      .where(sql`session_id IS NOT NULL`),
  ]
)

// Who is subscribed to an issue (D7). Auto-populated on create/assign/comment/
// mention; a `manual` row with `unsubscribed=true` suppresses auto-resubscribe.
// Drives both the inbox feed and the notification push fan-out.
// External widget reporters are modeled directly (no throwaway users row):
// `userId` null + `email` set + source='widget_reporter'.
export const issueSubscribers = pgTable(
  `issue_subscribers`,
  {
    id: uuidPk(),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    // Nullable: widget_reporter rows carry `email` instead.
    userId: text(`user_id`).references(() => users.id, {
      onDelete: `cascade`,
    }),
    // Set for widget_reporter rows; null for member rows.
    email: varchar({ length: 320 }),
    // Denormalized from issue→board by populate_issue_subscriber_team_id.
    // Scopes the Electric shape filter (team-stable, REV2-5) and serves the
    // notification fan-out and team-level queries.
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Denormalized from issue→board (populate_issue_child_board_id).
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    // Mirrors of the parent board's deleted_at / archived_at
    // (trigger-maintained, REV2-5 + EXP-500) — the shape's static trash and
    // archive predicates. Server-only, shape-excluded.
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    source: subscriberSourceEnum().notNull(),
    unsubscribed: boolean().notNull().default(false),
    ...timestamps,
  },
  (table) => [
    uniqueIndex(`uniq_issue_subscribers_user`)
      .on(table.issueId, table.userId)
      .where(sql`user_id IS NOT NULL`),
    uniqueIndex(`uniq_issue_subscribers_email`)
      .on(table.issueId, table.email)
      .where(sql`email IS NOT NULL`),
    // Plain (non-partial) on issue_id: the RI cascade's unconditional
    // `DELETE … WHERE issue_id = $1` can't use the partial uniques above (no
    // predicate implication), nor can issues.move's board_id rewrite.
    index(`idx_issue_subscribers_issue`).on(table.issueId),
    index(`idx_issue_subscribers_user`).on(table.userId),
    index(`idx_issue_subscribers_team`).on(table.teamId),
    index(`idx_issue_subscribers_board`).on(table.boardId),
  ]
)

// EXP-778: personal pins — the sidebar's "Pinned" group on every client. One
// row per (user, target); `kind` names the target column that is set
// (issue_id / session_id / action_id), each an FK cascade so a pin dies with
// its target. Team-scoped only for grouping: the shape is per USER (static
// `user_id = me`, never rotating), clients render the rows of the active team
// whose target they can resolve from the other shapes — a pin whose issue sits
// on a trashed board simply has no synced target until the board purges (48h,
// cascade) or restores. `sort_order` is fractional-index room for a future
// drag reorder; today a new pin appends.
export const pins = pgTable(
  `pins`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    kind: pinKindEnum().notNull(),
    issueId: uuid(`issue_id`).references(() => issues.id, {
      onDelete: `cascade`,
    }),
    sessionId: uuid(`session_id`).references(() => sessions.id, {
      onDelete: `cascade`,
    }),
    actionId: uuid(`action_id`).references(() => actions.id, {
      onDelete: `cascade`,
    }),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    check(
      `pins_kind_target_check`,
      sql`(kind = 'issue' AND issue_id IS NOT NULL AND session_id IS NULL AND action_id IS NULL) OR (kind = 'session' AND session_id IS NOT NULL AND issue_id IS NULL AND action_id IS NULL) OR (kind = 'action' AND action_id IS NOT NULL AND issue_id IS NULL AND session_id IS NULL)`
    ),
    uniqueIndex(`uniq_pins_user_issue`)
      .on(table.userId, table.issueId)
      .where(sql`issue_id IS NOT NULL`),
    uniqueIndex(`uniq_pins_user_session`)
      .on(table.userId, table.sessionId)
      .where(sql`session_id IS NOT NULL`),
    uniqueIndex(`uniq_pins_user_action`)
      .on(table.userId, table.actionId)
      .where(sql`action_id IS NOT NULL`),
    // Plain indexes on every FK: the RI cascades' unconditional deletes can't
    // use the partial uniques above.
    index(`idx_pins_user`).on(table.userId),
    index(`idx_pins_team`).on(table.teamId),
    index(`idx_pins_issue`).on(table.issueId),
    index(`idx_pins_session`).on(table.sessionId),
    index(`idx_pins_action`).on(table.actionId),
  ]
)




// ---------------------------------------------------------------------------
// NEW-1: ONE session tree, root = worker
// ---------------------------------------------------------------------------
// A thread IS a root session (`parent_id` NULL); children are sub-sessions
// on any device. Every session does its own work (the composer's model and
// plan mode, a worktree when `repository_id` is set) and spawns children
// only when it fans out; there is no dispatcher. The thread facets live on
// the root (`title`, `guide`; its issues = the UNION of the tree's
// `session_issues`, its PRs via `pull_requests`). Every start and message
// is `sessions.send` and the DATABASE is the bus: a new session is a
// `queued` row naming its device, which picks it up off the sessions shape;
// the SERVER posts every child transition into the parent's conversation
// (`lib/sessions/bus.ts`) and into every live session whose `wake_on`
// names the child, and a watchdog (`lib/sessions/watchdog.ts`) flags what
// stalls.
//
// NEW-2: a team AGENT is a definition row (`agents`, below) and RUNS as
// conversation sessions: one root per (agent, member) = that member's
// private DM with it (`agent_id` set, `user_id` = the member, the agent's
// device and login), never ending. The former agent facets (`kind`, `name`,
// `icon`, `color`, `triggers`, `sort_order`) and the action facets
// (`action_*`) stay as LEGACY read shims for the iOS store build and the
// ≤0.14.74 devices: a conversation session mirrors `kind = agent` + the
// agent's name/icon/color so those devices keep it alive (persistent) and
// those clients list it; nothing else writes them. NEW-8 drops them.
//
// Visibility (ONE rule ×4): `shared` is stamped at creation = the device is
// shared with the team (a child inherits its root's). Shared rows sync to
// every team member (read, post, answer, Stop); unshared rows to the starter
// only. The shape = `team_id IN (member teams) AND (shared OR user_id = me)`;
// every child table carries `shared` + `owner_id` mirrors so its shape stays
// single-table. The account that RUNS a session is derived from the devices
// rows (`lib/sessions/host.ts`): the starter's own row for `device_id`, else
// the row sharing the team.
export const sessions = pgTable(
  `sessions`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // The tree. NULL parent = a root. `root_id` = the root's id (its own on
    // a root), written by the app at insert; `depth` = 0 on a root
    // (follow-up cap: contract sessionsSend.treeMaxDepth).
    parentId: uuid(`parent_id`).references((): AnyPgColumn => sessions.id, {
      onDelete: `cascade`,
    }),
    rootId: uuid(`root_id`)
      .notNull()
      .references((): AnyPgColumn => sessions.id, { onDelete: `cascade` }),
    depth: integer().notNull().default(0),
    // LEGACY read shim (NEW-2): `agent` on a conversation session (derived
    // from `agent_id`, for the ≤0.14.74 devices' persistence), else `thread`.
    kind: sessionKindEnum().notNull().default(`thread`),
    // NEW-2: the agent this is a conversation session OF (a root; one per
    // (agent, user_id)); its children carry nothing (they resolve through
    // the root). NULL = a thread.
    agentId: uuid(`agent_id`).references((): AnyPgColumn => agents.id, {
      onDelete: `cascade`,
    }),
    // #EXP-1383: a ROOT a team agent's session started (a thread its
    // conversation spawned): the tree acts as that agent and stays in its
    // team like the conversation itself (acting agent = `agent_id ??
    // started_by_agent_id`). SYNCED.
    startedByAgentId: uuid(`started_by_agent_id`).references(
      (): AnyPgColumn => agents.id,
      { onDelete: `set null` }
    ),
    // VAPP-110: the APP this is a member's EDITOR session OF (a root; one
    // per (app, user_id), live only while that member has the editor open:
    // leaving ends the tree, the next message revives the same root). NULL = not an
    // editor session.
    appId: uuid(`app_id`).references((): AnyPgColumn => apps.id, {
      onDelete: `cascade`,
    }),
    // ── runner ──
    // The STARTER (the real person; a teammate's start on a shared server
    // device stays theirs). Visibility keys on it.
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // The steer deviceId of the machine that runs it (no FK: devices rows
    // are per (user, deviceId)). The device picks up `queued` rows naming it.
    deviceId: varchar(`device_id`, { length: 128 }).notNull(),
    // The agent CLI (contract codingAgent), its account profile, model,
    // effort and launch switches — the composer's pickers, fixed at start;
    // a child inherits its parent's.
    cli: varchar({ length: 16 }),
    account: varchar({ length: 64 }),
    model: varchar({ length: 64 }),
    effort: varchar({ length: 32 }),
    planMode: boolean(`plan_mode`).notNull().default(false),
    computerUse: boolean(`computer_use`).notNull().default(false),
    ultracode: boolean().notNull().default(false),
    // LEGACY read shim (NEW-1): the dispatcher flag. Never true any more;
    // synced for the iOS store build and the ≤0.14.74 devices. NEW-8 drops it.
    dispatch: boolean().notNull().default(false),
    // SERVER-ONLY (EXP-1140): the team MCP servers this session picked —
    // the only ids `mcpServers.resolveForLaunch` hands credentials for.
    mcpServerIds: jsonb(`mcp_server_ids`).$type<string[]>(),
    // SERVER-ONLY (NEW-1): the sessions this one WATCHES. The bus delivers
    // every transition of a watched session here exactly as to its parent,
    // until the watched session ends (`sessions.send({wakeOn})`; a root
    // started from inside a session is watched by its starter).
    wakeOn: jsonb(`wake_on`).$type<string[]>(),
    repositoryId: uuid(`repository_id`).references(() => repositories.id, {
      onDelete: `set null`,
    }),
    // The session's working branch (device-written at claim).
    branch: varchar({ length: 255 }),
    // LEGACY read shims (NEW-1): the EXP-1252 move. A move is now an end
    // here and a continuation on the target; nothing writes these. NEW-8
    // drops them.
    moveToDeviceId: varchar(`move_to_device_id`, { length: 128 }),
    handoffRestoredAt: timestamp(`handoff_restored_at`, { withTimezone: true }),
    // ── state ──
    status: sessionStatusEnum().notNull().default(`queued`),
    // Device-written turn state (EXP-848): the ONLY list-spinner input ×4.
    busy: boolean().notNull().default(false),
    // EXP-1374: busy ONLY because background tasks (shells, subagents,
    // workflows) still run after the turn ended (device-written); the slot
    // stays occupied, the card reads "Background task".
    background: boolean().notNull().default(false),
    // A question/plan card waits for a person (the "Needs you" group); the
    // server also holds it true while `blocked` is set.
    needsInput: boolean(`needs_input`).notNull().default(false),
    // Why the session cannot proceed (contract codingSessionBlocked.kinds):
    // the usage wall (device-written), or the watchdog's `device_offline`,
    // `unclaimed`, `stuck`. A blocked session stays live.
    blocked: jsonb().$type<CodingSessionBlocked>(),
    startedAt: timestamp(`started_at`, { withTimezone: true }),
    lastTurnAt: timestamp(`last_turn_at`, { withTimezone: true }),
    // EXP-1374: the latest sign of life beyond a row write (SERVER-ONLY):
    // the heartbeat's per-session activity (a tool call, terminal output),
    // an MCP call by the session, a push to its branch. The stuck watchdog
    // measures from greatest(updated_at, last_activity_at).
    lastActivityAt: timestamp(`last_activity_at`, { withTimezone: true }),
    // EXP-1413 (SERVER-ONLY): a person's Stop interrupted a busy turn; set
    // until the session works again. The yolo automerge waits on it, so a
    // stopped turn's half-done work never merges.
    stoppedAt: timestamp(`stopped_at`, { withTimezone: true }),
    endedAt: timestamp(`ended_at`, { withTimezone: true }),
    // ── visibility ── (stamped at creation, never changes)
    shared: boolean().notNull().default(false),
    // ── thread facets (roots) ──
    // Set by a person or the agent; NULL = derived for display
    // (session-title.json). On a child: the CLI's auto-name (EXP-905).
    title: varchar({ length: 255 }),
    // LEGACY read shims (NEW-1): the EXP-1323 live status line and the
    // `/btw` aside flag. Never written; NEW-8 drops them.
    caption: varchar({ length: 80 }),
    aside: boolean().notNull().default(false),
    // The thread's ONE Guide = every PR body (`guide_write`, FOR UPDATE).
    guide: jsonb().$type<GuideEntry[]>(),
    // LEGACY read shims (NEW-2): the action a thread ran. A prompt run
    // (`sessions.send({promptId})`) composes the body into the first message
    // and names the session after the prompt instead; never written. NEW-8
    // drops them.
    actionId: uuid(`action_id`).references(() => actions.id, {
      onDelete: `set null`,
    }),
    actionName: varchar(`action_name`, { length: 255 }),
    actionInputs: jsonb(`action_inputs`).$type<Record<string, string>>(),
    // LEGACY read shim (NEW-1): the session that spawned this root. The
    // bus routes a spawned root to its spawner through `wake_on` instead;
    // never written (the ≤0.14.74 desktop names it in its shape request).
    // NEW-8 drops it.
    spawnedBySessionId: uuid(`spawned_by_session_id`),
    // LEGACY read shims (NEW-2): the agent facets, now on `agents`. A
    // conversation session mirrors its agent's name/icon/color (stamped at
    // creation, re-stamped by `agents.update`); `triggers` stays `[]`.
    name: varchar({ length: 64 }),
    icon: varchar({ length: 64 }),
    color: varchar({ length: 7 }),
    triggers: jsonb()
      .$type<AgentTrigger[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    // #EXP-1383: when this session's device last fetched its brief
    // (`sessions.brief` stamps it): the agent instructions it runs on are
    // the ones of that moment. SYNCED (chats.json `agentHint`).
    agentInstructionsAt: timestamp(`agent_instructions_at`, {
      withTimezone: true,
    }),
    ...timestamps,
  },
  (table) => [
    // One conversation per (agent, member).
    uniqueIndex(`uniq_sessions_agent_conversation`)
      .on(table.agentId, table.userId)
      .where(sql`agent_id IS NOT NULL AND parent_id IS NULL`),
    index(`idx_sessions_agent`).on(table.agentId),
    // VAPP-110: one editor session per (app, member).
    uniqueIndex(`uniq_sessions_app_editor`)
      .on(table.appId, table.userId)
      .where(sql`app_id IS NOT NULL AND parent_id IS NULL`),
    index(`idx_sessions_app`).on(table.appId),
    check(
      `sessions_root_check`,
      sql`(parent_id IS NULL AND depth = 0 AND root_id = id) OR (parent_id IS NOT NULL AND depth > 0)`
    ),
    index(`idx_sessions_team`).on(table.teamId),
    index(`idx_sessions_parent`).on(table.parentId),
    index(`idx_sessions_root`).on(table.rootId),
    index(`idx_sessions_user`).on(table.userId),
    index(`idx_sessions_action`).on(table.actionId),
    // The bus's reverse lookup: who watches a session.
    index(`idx_sessions_wake_on`).using(`gin`, table.wakeOn),
    // The device pickup + the watchdog.
    index(`idx_sessions_device_live`)
      .on(table.deviceId, table.status)
      .where(sql`status <> 'ended'`),
  ]
)

// NEW-2: a team AGENT = a definition (WHO, HOW, WHEN), never a session.
// Name, icon and color like a board; server-only `instructions` (every turn's
// system append in its conversations and their children); `triggers` (a
// cron schedule or an event name, evaluated by the SERVER each watchdog
// tick, `lib/agents/triggers.ts`) with their server-only `trigger_state`;
// the runner: `device_id` + `cli` + `account` + `model` (the composer's
// picks, fixed per agent) and `owner_id` = the member whose conversation
// session a trigger wakes. Owner-written (`agents` router + MCP
// `agents_list`), member-read; synced MINUS `instructions`/`trigger_state`.
// Deleting an agent cascades its conversation sessions (and their trees).
export const agents = pgTable(
  `agents`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    name: varchar({ length: 64 }).notNull(),
    icon: varchar({ length: 64 }),
    color: varchar({ length: 7 }),
    // SERVER-ONLY (tRPC `agents.get`).
    instructions: text().notNull().default(``),
    // #EXP-1383: when `instructions` last CHANGED (create + an update that
    // edits the text; nothing else). SYNCED: the chat's "Instructions
    // changed" hint compares it with `sessions.agent_instructions_at`.
    instructionsUpdatedAt: timestamp(`instructions_updated_at`, {
      withTimezone: true,
    }),
    triggers: jsonb()
      .$type<AgentTrigger[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    // SERVER-ONLY: per-trigger bookkeeping (last evaluated tick, last fire).
    triggerState: jsonb(`trigger_state`)
      .$type<AgentTriggerState>()
      .notNull()
      .default(sql`'{}'::jsonb`),
    deviceId: varchar(`device_id`, { length: 128 }).notNull(),
    cli: varchar({ length: 16 }),
    account: varchar({ length: 64 }),
    model: varchar({ length: 64 }),
    ownerId: text(`owner_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    index(`idx_agents_team`).on(table.teamId),
    index(`idx_agents_device`).on(table.deviceId),
  ]
)

// NEW-2: EVENTS, the team's event stream (synced, listed on the Agents
// page). A row = a NAME (free text: the server builtins
// `issue.created` / `issue.status_changed` / `pr.opened` / `pr.merged` /
// `session.ended`, or anything an agent emits over MCP `events_emit`, e.g.
// `ui-changed`) + a jsonb payload + who emitted it (a session, a person, or
// the server). An agent trigger `{kind: event, name}` makes the server post
// the event into the agent owner's conversation session as it lands
// (`lib/events.ts`); an agent never fires on its own emit. Rows older than
// contract `events.retentionDays` are pruned by the watchdog.
export const eventEmitterEnum = pgEnum(`event_emitter`, eventEmitterValues)
export const events = pgTable(
  `events`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    name: varchar({ length: 128 }).notNull(),
    payload: jsonb().$type<Record<string, unknown>>().notNull().default(sql`'{}'::jsonb`),
    emittedBy: eventEmitterEnum(`emitted_by`).notNull().$type<EventEmitter>(),
    sessionId: uuid(`session_id`).references((): AnyPgColumn => sessions.id, {
      onDelete: `set null`,
    }),
    userId: text(`user_id`).references(() => users.id, { onDelete: `set null` }),
    createdAt: timestamp(`created_at`, { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    index(`idx_events_team_created`).on(table.teamId, table.createdAt),
    index(`idx_events_team_name`).on(table.teamId, table.name),
  ]
)

// NEW-2: PROMPTS replace actions: a name, a curated icon and a markdown
// body, nothing else (no inputs, triggers, repository or pick types). They
// live in the composer's `/` menu and Settings → Prompts; running one =
// `sessions.send({promptId, text})`: a root whose first message is the body
// plus what was typed, titled after the prompt. Synced MINUS `body` (tRPC
// `prompts.get` / MCP `prompts_get`); owner-written, member-read.
export const prompts = pgTable(
  `prompts`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    icon: varchar({ length: 64 }),
    body: text().notNull(),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.name),
    index(`idx_prompts_team`).on(table.teamId),
  ]
)

// VAPP-110: Exponential APPS. An app is a DECLARATIVE PACKAGE (the Exponential
// UI SDK's package format: a manifest + A2UI templates + bindings + a
// function allowlist, data never code) owned by a team and rendered inside
// every client by the Exponential UI renderers through the app host. The
// row carries the HEAD version's package (synced, so the editor's live
// preview follows every upsert; NULL until the first version lands);
// `app_versions` keeps every version (no git: the DB is the history).
// Authoring in slice 1 is agents only: a member's EDITOR session
// (`sessions.app_id`) writes versions over MCP `apps upsert`. `deleted_at`
// = the trash mirror of the apps shape. Contract caps: `contract.apps`.
export type AppPackageJson = Record<string, unknown>
export const apps = pgTable(
  `apps`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // The handle in URLs and MCP calls (`/t/<team>/apps/<slug>`), unique per team.
    slug: varchar({ length: 64 }).notNull(),
    name: varchar({ length: 64 }).notNull(),
    // The board icon set (a curated glyph + colour), like boards and agents.
    icon: varchar({ length: 64 }),
    color: varchar({ length: 7 }),
    description: text().notNull().default(``),
    // The HEAD version's package; NULL = no version yet.
    package: jsonb().$type<AppPackageJson>(),
    headVersion: integer(`head_version`).notNull().default(0),
    // Release train 2026-10-23: apps are shared team content, so a deleted
    // account leaves its apps behind (NULL creator = owner-only to manage).
    createdBy: text(`created_by`).references(() => users.id, { onDelete: `set null` }),
    // VAPP-120: the typed host actions (`exponential.<noun>.<op>`, exact
    // names) the app may run as the viewer WITHOUT the consent card; the
    // creator or an owner grants them on the app's settings page.
    grantedTools: jsonb(`granted_tools`).$type<string[]>().notNull().default([]),
    deletedAt: timestamp(`deleted_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    // Release train 2026-10-23: a trashed app frees its slug.
    uniqueIndex(`uniq_apps_team_slug`)
      .on(table.teamId, table.slug)
      .where(sql`deleted_at IS NULL`),
    index(`idx_apps_team`).on(table.teamId),
  ]
)

// VAPP-110: every version of an app's package, numbered from 1 per app. A
// row is written by `apps upsert` (MCP, from the member's editor session)
// or by Restore (a new head copying an older one). `package` stays BEHIND
// the shape (tRPC `apps.version` / MCP `apps get {version}` on demand); the
// head's package rides on `apps`.
export const appVersions = pgTable(
  `app_versions`,
  {
    id: uuidPk(),
    appId: uuid(`app_id`)
      .notNull()
      .references(() => apps.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    number: integer().notNull(),
    package: jsonb().$type<AppPackageJson>().notNull(),
    // What changed, one line (the agent's, or `Restored v3`).
    note: text(),
    // The editor session that wrote it (NULL = a person's restore).
    sessionId: uuid(`session_id`).references(() => sessions.id, { onDelete: `set null` }),
    createdBy: text(`created_by`).references(() => users.id, { onDelete: `set null` }),
    createdAt: timestamp(`created_at`, { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    unique().on(table.appId, table.number),
    index(`idx_app_versions_app`).on(table.appId),
    index(`idx_app_versions_team`).on(table.teamId),
  ]
)

// VAPP-122: an app's RECORDS, its own data: one JSON object (`data`) of a
// `kind` the app's manifest declares (`records: {<kind>: {schema}}`, a JSON
// Schema subset the SDK validates), owned by the TEAM under the app. One
// synced shape `app-records` (member-only, team-scoped); `deleted_at` and
// the trigger-kept `app_deleted_at` mirror of the app's trash are the
// shape's STATIC predicates, so trash never reaches a client. Reads = the
// `exp:records?kind=…` binding every app host resolves; writes = tRPC
// `apps.records.*` (the host functions `harness.records.*`, MCP `apps
// records_*`), validated against the kind's schema, capped by contract
// `apps.recordMaxBytes` and the team's plan quota (`appRecords`).
export type AppRecordData = Record<string, unknown>
export const appRecords = pgTable(
  `app_records`,
  {
    id: uuidPk(),
    appId: uuid(`app_id`)
      .notNull()
      .references(() => apps.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    kind: varchar({ length: 32 }).notNull(),
    data: jsonb().$type<AppRecordData>().notNull(),
    createdBy: text(`created_by`).references(() => users.id, { onDelete: `set null` }),
    deletedAt: timestamp(`deleted_at`, { withTimezone: true }),
    // Mirror of the app's deleted_at (trigger-maintained): the shape's static
    // trash predicate. Server-only, shape-excluded.
    appDeletedAt: timestamp(`app_deleted_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_app_records_app_kind`).on(table.appId, table.kind),
    index(`idx_app_records_team`).on(table.teamId),
  ]
)

// A session's issues (n ↔ n: an issue may sit in several sessions). NEW-1:
// a row sits on the session that HOLDS the issue (a worker's link, the
// issues a start names, a filed issue); a forward moves the row; a root's
// list = the union over its tree. Carries the issue's board mirrors
// (trash/archive, REV2-5) and the holder's visibility mirrors (`shared`,
// `owner_id`), all trigger-maintained.
export const sessionIssues = pgTable(
  `session_issues`,
  {
    id: uuidPk(),
    sessionId: uuid(`session_id`)
      .notNull()
      .references(() => sessions.id, { onDelete: `cascade` }),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Server-only mirrors (shape-excluded; the where clause filters on them).
    boardId: uuid(`board_id`).references(() => boards.id, {
      onDelete: `cascade`,
    }),
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    shared: boolean().notNull().default(false),
    ownerId: text(`owner_id`).references(() => users.id, {
      onDelete: `cascade`,
    }),
    source: sessionIssueSourceEnum().notNull().default(`user`),
    addedBy: text(`added_by`).references(() => users.id, {
      onDelete: `set null`,
    }),
    // Provenance: the session whose agent linked it (never its location).
    addedBySessionId: uuid(`added_by_session_id`).references(
      (): AnyPgColumn => sessions.id,
      { onDelete: `set null` }
    ),
    // LEGACY read shim (NEW-1): the EXP-1341 assignment. Migrated into
    // `session_id`; never written; the ≤0.14.74 desktop names it in its
    // shape request. NEW-8 drops it.
    assignedSessionId: uuid(`assigned_session_id`).references(
      (): AnyPgColumn => sessions.id,
      { onDelete: `set null` }
    ),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    unique().on(table.sessionId, table.issueId),
    index(`idx_session_issues_issue`).on(table.issueId),
    index(`idx_session_issues_team`).on(table.teamId),
    index(`idx_session_issues_board`).on(table.boardId),
    index(`idx_session_issues_added_by_session`).on(table.addedBySessionId),
  ]
)

// NEW-3: CONVERSATIONS, the chat primitive (plan 3.9). A row = where
// messages live: kind `session` = a session's own conversation (EVERY
// session has exactly one, created by the `create_session_conversation`
// trigger, `session_id` set; an agent DM IS the agent's conversation session
// for that member, NEW-2), `dm` / `channel` = people talking to people
// (step 2, NEW-8: `session_id` NULL, members in `conversation_members`).
// Visibility: a session conversation follows the session rule through the
// mirrored `shared` / `owner_id` (trigger-written, immutable like a
// message's), a dm/channel = its members. Synced (`/api/shapes/conversations`).
/** #EXP-1380: `conversations.digest` (domain-contract `ConversationDigest`). */
export interface ConversationDigestJson {
  first: ConversationDigestMessageJson | null
  preview: ConversationDigestMessageJson | null
  waiting: ConversationDigestMessageJson | null
  agentAt: string | null
  agentTimes: string[]
}
export interface ConversationDigestMessageJson {
  id: string
  authorKind: string
  authorSessionId: string | null
  body: string
  card: { kind: string; question?: string; plan?: string; title?: string } | null
  answeredAt: string | null
  createdAt: string
}

export const conversations = pgTable(
  `conversations`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    kind: conversationKindEnum().notNull().$type<ConversationKind>(),
    // The session this conversation belongs to (kind `session`), one each.
    sessionId: uuid(`session_id`).references((): AnyPgColumn => sessions.id, {
      onDelete: `cascade`,
    }),
    // A channel's name; NULL on a dm and a session conversation.
    title: varchar({ length: 255 }),
    // Mirrors of the session (trigger-written, immutable); NULL/false on a
    // dm/channel, whose readers are its members.
    shared: boolean().notNull().default(false),
    ownerId: text(`owner_id`).references(() => users.id, { onDelete: `cascade` }),
    // #EXP-1380: the list's digest of the messages (trigger-written,
    // `conversation_digest`; domain-contract `conversationDigest`), so the
    // web list never syncs a transcript. NULL until the first boot fills it.
    digest: jsonb().$type<ConversationDigestJson>(),
    ...timestamps,
  },
  (table) => [
    uniqueIndex(`uniq_conversations_session`).on(table.sessionId),
    index(`idx_conversations_team`).on(table.teamId),
  ]
)

// NEW-3: who is in a dm/channel (step 2 writes rows; a session conversation
// has none: the session rule decides). SERVER-ONLY until NEW-8.
export const conversationMembers = pgTable(
  `conversation_members`,
  {
    conversationId: uuid(`conversation_id`)
      .notNull()
      .references(() => conversations.id, { onDelete: `cascade` }),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    createdAt: timestamp(`created_at`, { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    primaryKey({ columns: [table.conversationId, table.userId] }),
    index(`idx_conversation_members_user`).on(table.userId),
  ]
)

// The Conversation: one row per person message, per agent turn reply (the
// turn's FINAL text, written by the device), per card. The raw tool
// transcript stays on the device (journal + relay) and shows only when a
// session is opened. A person's message is the bus: the device delivers it
// into the session (resuming an idle one) and stamps `consumed_at`.
export const messages = pgTable(
  `messages`,
  {
    id: uuidPk(),
    // NEW-3: the conversation the row lives in (kind `session` in step 1:
    // the session's own; `populate_message_session_mirrors` derives it from
    // `session_id` and vice versa, so either suffices at insert).
    conversationId: uuid(`conversation_id`)
      .notNull()
      .references(() => conversations.id, { onDelete: `cascade` }),
    // NEW-3: the step-1 read shim of the session's conversation (the shipped
    // clients key the Conversation on it); NEW-8 drops it with the shim.
    sessionId: uuid(`session_id`)
      .notNull()
      .references(() => sessions.id, { onDelete: `cascade` }),
    // Mirrors of the session (app-written at insert, immutable).
    rootId: uuid(`root_id`)
      .notNull()
      .references(() => sessions.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    shared: boolean().notNull().default(false),
    ownerId: text(`owner_id`).references(() => users.id, {
      onDelete: `cascade`,
    }),
    authorKind: messageAuthorKindEnum(`author_kind`).notNull(),
    // The person (author_kind user); NULL for agent/system rows.
    authorId: text(`author_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    // EXP-1323: the SESSION that posted it (a `sessions_send` through a
    // session's MCP key, a child's `report`); NULL = a person or the server.
    authorSessionId: uuid(`author_session_id`).references(
      (): AnyPgColumn => sessions.id,
      { onDelete: `set null` }
    ),
    // Plain GFM (images `![alt](/api/attachments/{id})`, mentions `@email`,
    // refs `#IDENT`). Empty on a pure card.
    body: text().notNull().default(``),
    card: jsonb().$type<MessageCard>(),
    // A question/plan card's answer time (the answer lives in `card`).
    answeredAt: timestamp(`answered_at`, { withTimezone: true }),
    // A person's message: when the device delivered it into the agent.
    // NULL = queued (the queue bar ×4).
    consumedAt: timestamp(`consumed_at`, { withTimezone: true }),
    // #NEW-57: a meeting transcript batch (`sessions.send {meeting: true}`)
    // expires `contract.meeting.retentionDays` after it lands; the watchdog
    // deletes it then. NULL = kept. SERVER-ONLY: never in the shape's columns.
    expiresAt: timestamp(`expires_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    // #NEW-57: the watchdog's prune.
    index(`idx_messages_expires`).on(table.expiresAt).where(sql`expires_at IS NOT NULL`),
    index(`idx_messages_session_created`).on(table.sessionId, table.createdAt),
    index(`idx_messages_conversation_created`).on(table.conversationId, table.createdAt),
    index(`idx_messages_root`).on(table.rootId),
    index(`idx_messages_team`).on(table.teamId),
    // The device's pickup of a PERSON's queued messages. EXP-1323 widened
    // the pending set (`pendingInput()`: `author_kind = 'user' OR
    // author_session_id IS NOT NULL`); another session's input rides
    // `idx_messages_author_session` instead, so this predicate stays as is.
    index(`idx_messages_pending`)
      .on(table.sessionId)
      .where(sql`author_kind = 'user' AND consumed_at IS NULL`),
    index(`idx_messages_author_session`).on(table.authorSessionId),
    // #EXP-1399: one `ask` mirror of a card per conversation (escalate
    // twice, or the bus and escalate, never insert twice).
    uniqueIndex(`uq_messages_ask_mirror`)
      .on(table.sessionId, sql`(card->>'messageId')`)
      .where(sql`card->>'kind' = 'ask'`),
  ]
)

// EXP-1323: per-user read state of a session (a thread = its root's row).
// Opening a thread or a session upserts `read_at` (`sessions.markRead`); a
// list row is bold while its tree's newest agent message is newer. Synced
// like `pins`: a STATIC per-user shape (`user_id = me`), never trash-scoped;
// a row renders only beside a synced session.
export const sessionReads = pgTable(
  `session_reads`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    sessionId: uuid(`session_id`)
      .notNull()
      .references(() => sessions.id, { onDelete: `cascade` }),
    readAt: timestamp(`read_at`, { withTimezone: true }).notNull().defaultNow(),
    // #EXP-1388: the viewer archived the chat (on the ROOT's row; NULL = in
    // the list). Per viewer, like the read; `sessions.setArchived`.
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    uniqueIndex(`uniq_session_reads_user_session`).on(table.userId, table.sessionId),
    index(`idx_session_reads_session`).on(table.sessionId),
  ]
)

// A thread's pull requests (n: a tree, a stack or independent). All PR and
// branch state lives here, never on `issues`. Team-visible (an issue's PR
// badge is team data; GitHub shows it anyway). `base_pull_request_id` = the
// open PR this one is stacked on (native GitHub stacks).
export const pullRequests = pgTable(
  `pull_requests`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    rootSessionId: uuid(`root_session_id`).references(() => sessions.id, {
      onDelete: `set null`,
    }),
    // The session that opened it (`pr_open`).
    sessionId: uuid(`session_id`).references(() => sessions.id, {
      onDelete: `set null`,
    }),
    repositoryId: uuid(`repository_id`)
      .notNull()
      .references(() => repositories.id, { onDelete: `cascade` }),
    number: integer().notNull(),
    url: text().notNull(),
    title: varchar({ length: 500 }),
    head: varchar({ length: 255 }).notNull(),
    base: varchar({ length: 255 }).notNull(),
    basePullRequestId: uuid(`base_pull_request_id`).references(
      (): AnyPgColumn => pullRequests.id,
      { onDelete: `set null` }
    ),
    state: prStateEnum().notNull().default(`open`),
    mergedAt: timestamp(`merged_at`, { withTimezone: true }),
    // FEED-160: GitHub's `updated_at` of the last PR state (draft/open) we
    // applied; a generic delivery older than it never flips the draft flag.
    // `updated_at` moves on any write, so it cannot guard. Server-only.
    githubUpdatedAt: timestamp(`github_updated_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.repositoryId, table.number),
    uniqueIndex(`uniq_pull_requests_url`).on(table.url),
    index(`idx_pull_requests_team`).on(table.teamId),
    index(`idx_pull_requests_root`).on(table.rootSessionId),
    index(`idx_pull_requests_session`).on(table.sessionId),
    index(`idx_pull_requests_head`).on(table.repositoryId, table.head),
    index(`idx_pull_requests_base_pr`).on(table.basePullRequestId),
  ]
)

// Which issues a PR closes (n ↔ n). Carries the issue's board mirrors.
export const pullRequestIssues = pgTable(
  `pull_request_issues`,
  {
    id: uuidPk(),
    pullRequestId: uuid(`pull_request_id`)
      .notNull()
      .references(() => pullRequests.id, { onDelete: `cascade` }),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    boardId: uuid(`board_id`).references(() => boards.id, {
      onDelete: `cascade`,
    }),
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.pullRequestId, table.issueId),
    index(`idx_pull_request_issues_issue`).on(table.issueId),
    index(`idx_pull_request_issues_team`).on(table.teamId),
    index(`idx_pull_request_issues_board`).on(table.boardId),
  ]
)

// EXP-878: issue drafts — what the create-issue dialog keeps when it is
// closed with content in it, on every client. Per USER like pins (the shape
// is the static `user_id = me`, never team- or trash-scoped; a row renders
// only when its board resolves from the boards shape). The id is minted by
// the CLIENT (a fresh uuid per dialog session, the row's id when reopened)
// so a close is one idempotent upsert and create can delete the draft in the
// same transaction. `description` is GFM whose images are already final
// `/api/attachments/{id}` URLs: uploads are eager into draft-owned
// attachments rows (`attachments.draft_id`), reparented on create.
// `label_ids` carries no FK; clients drop ids they cannot resolve.
export const issueDrafts = pgTable(
  `issue_drafts`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    title: text().notNull().default(``),
    description: text().notNull().default(``),
    // NULL = the team's Backlog builtin.
    statusId: uuid(`status_id`).references(() => issueStatuses.id, {
      onDelete: `set null`,
    }),
    priority: issuePriorityEnum().notNull().default(`none`),
    assigneeId: text(`assignee_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    // #EXP-1383: an agent assignee (exclusive with `assignee_id`).
    assigneeAgentId: uuid(`assignee_agent_id`).references(
      (): AnyPgColumn => agents.id,
      { onDelete: `set null` }
    ),
    labelIds: uuid(`label_ids`)
      .array()
      .notNull()
      .default(sql`'{}'::uuid[]`),
    dueDate: date(`due_date`),
    // EXP-1284: the thread a "New issue in this thread" draft files into
    // (`issues.create` consuming the draft links the issue there). SET NULL:
    // a deleted thread leaves a plain draft.
    sessionId: uuid(`session_id`).references(() => sessions.id, {
      onDelete: `set null`,
    }),
    ...timestamps,
  },
  (table) => [
    index(`idx_issue_drafts_user`).on(table.userId),
    index(`idx_issue_drafts_team`).on(table.teamId),
    index(`idx_issue_drafts_board`).on(table.boardId),
    check(
      `issue_drafts_single_assignee_check`,
      sql`assignee_id IS NULL OR assignee_agent_id IS NULL`
    ),
  ]
)

// Activity log (D9): status/assignee/label/PR/plan/error events, rendered as a
// Linear-style timeline on every client. `payload` carries event-specific data
// (e.g. { from, to } for a status change).
export const issueEvents = pgTable(
  `issue_events`,
  {
    id: uuidPk(),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    // Denormalized from issue→board by populate_issue_event_team_id.
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Denormalized from issue→board (populate_issue_child_board_id).
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    // Mirrors of the parent board's deleted_at / archived_at
    // (trigger-maintained, REV2-5 + EXP-500) — the shape's static trash and
    // archive predicates. Server-only, shape-excluded.
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    actorUserId: text(`actor_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    // #EXP-1383: the agent whose session acted (`actor_user_id` keeps the
    // member behind it); the timeline names the agent. SYNCED.
    actorAgentId: uuid(`actor_agent_id`).references(
      (): AnyPgColumn => agents.id,
      { onDelete: `set null` }
    ),
    // FEED-87: who acted — `via_mcp` = an MCP call (an agent), `session_id`
    // = the coding run that made it (X-Exp-Session-Id; no FK, runs purge).
    // Server-only, shape-excluded; read by exponential_issues_activity.
    viaMcp: boolean(`via_mcp`).notNull().default(false),
    sessionId: uuid(`session_id`),
    type: issueEventTypeEnum().notNull(),
    payload: jsonb(),
    ...timestamps,
  },
  (table) => [
    index(`idx_issue_events_issue`).on(table.issueId),
    index(`idx_issue_events_team`).on(table.teamId),
    index(`idx_issue_events_board`).on(table.boardId),
  ]
)

// Team repository registry (SERVER-ONLY, tRPC-managed — never an Electric
// shape). One row per connected GitHub repo; the desktop "Start coding"
// launcher resolves its clone target through the board's `repositoryId`.
// GitHub itself stays storage-free (App JWT → JIT installation token on demand).
export const repositories = pgTable(
  `repositories`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // `owner/name` as GitHub reports it.
    fullName: text(`full_name`).notNull(),
    defaultBranch: text(`default_branch`).notNull().default(`main`),
    // Team-chosen default branch (EXP-462, e.g. `develop` while GitHub's
    // default stays `master`). NULL = follow GitHub. When set, it wins
    // everywhere the product means "the default branch" — the heal passes keep
    // maintaining `default_branch` underneath it.
    defaultBranchOverride: text(`default_branch_override`),
    private: boolean().notNull().default(false),
    // Cached GitHub App installation id; nullable — the App JWT can still
    // resolve it on demand (github-app.ts is storage-free).
    installationId: bigint(`installation_id`, { mode: `number` }),
    // The App lost access to this repo (installation_repositories webhook, or a
    // verified token mint failed). NULL = accessible as far as we know. Cleared
    // by connect, a webhook re-grant, and the list heal pass.
    inaccessibleAt: timestamp(`inaccessible_at`, { withTimezone: true }),
    // EXP-557 per-user sharing: the member whose GitHub connection put this
    // repo into the team. The sharer and team owners manage the row (remove,
    // branch pin); everyone can code on it. SET NULL (account deletion) and
    // pre-EXP-557 rows stay NULL = managed by owners only.
    sharedByUserId: text(`shared_by_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.fullName),
    index(`idx_repositories_team`).on(table.teamId),
    // EXP-835: the admin console's per-user correlated subqueries filter on
    // the sharer alone — without this they seq-scan the whole registry.
    index(`idx_repositories_shared_by`).on(table.sharedByUserId),
  ]
)

// EXP-792: team MCP servers a coding run may connect to besides Exponential's
// own `/api/mcp` — the ONE registry (no repo `.mcp.json`, no per-device
// servers). SERVER-ONLY (tRPC `mcpServers`, never a shape — the natives
// have no decode). The row itself is NON-SECRET: it names the headers / env
// variables a credential fills, never their values. Each MEMBER connects
// once and the server holds that member's credential encrypted in
// `mcp_credentials`, so it works on every device, remote start and
// automation. `auth`: `none` (connect as-is), `oauth` (the server runs
// discovery + PKCE and the hosted callback exchanges the code), `secret` (a
// value the member types, for the ONE declared header/env name).
// `transport`: `http` (url + header names) or `stdio` (command + args + env
// names). `scopes` is advisory (the OAuth scope request); `enabled_by_default`
// preselects the server in the launch pickers.
export const mcpServers = pgTable(
  `mcp_servers`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    name: varchar({ length: 64 }).notNull(),
    // Documented varchar (mcpTransportValues in domain.ts).
    transport: varchar({ length: 16 }).notNull().default(`http`),
    url: text(),
    headerNames: jsonb(`header_names`)
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    command: text(),
    args: jsonb().$type<string[]>().notNull().default(sql`'[]'::jsonb`),
    envNames: jsonb(`env_names`)
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    scopes: jsonb().$type<string[]>().notNull().default(sql`'[]'::jsonb`),
    // Documented varchar (mcpAuthValues in domain.ts).
    auth: varchar({ length: 16 }).notNull().default(`none`),
    enabledByDefault: boolean(`enabled_by_default`).notNull().default(false),
    createdById: text(`created_by_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.name),
    index(`idx_mcp_servers_team`).on(table.teamId),
  ]
)

// One member's credential for one team MCP server: the OAuth token set the
// hosted callback exchanged, or the secret they typed (`mcpServers.setSecret`).
// `ciphertext` = AES-256-GCM JSON {accessToken?, refreshToken?, tokenType?,
// tokenEndpoint?, value?} under a key derived from BETTER_AUTH_SECRET
// (lib/mcp-oauth/crypto.ts; rotating it = every member reconnects, a failed
// decrypt reads as not connected). `expires_at` = the OAuth access expiry,
// `issuer` + `client_id` = the client the tokens belong to (the refresh
// needs it), `error` = the last refresh failure. Server-only, never synced,
// never returned: `mcpServers.resolveForLaunch` hands the caller's OWN
// values to their launcher, and only for the servers its live run picked
// (`coding_sessions.mcp_server_ids`, EXP-1140). On a shared device the
// caller is the device OWNER, so a teammate's run there spends the owner's
// credentials, like its `expu_` key and GitHub token.
export const mcpCredentials = pgTable(
  `mcp_credentials`,
  {
    id: uuidPk(),
    serverId: uuid(`server_id`)
      .notNull()
      .references(() => mcpServers.id, { onDelete: `cascade` }),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    ciphertext: text().notNull(),
    expiresAt: timestamp(`expires_at`, { withTimezone: true }),
    issuer: text(),
    clientId: text(`client_id`),
    error: text(),
    // FEED-73: the member's own toggle (`mcpServers.setShared`). A shared
    // credential is spent by every run of an ACTION in this team whose list
    // (`actions.mcp_server_ids`) names the server, as an extra
    // `<server>-as-<member>` entry. Disconnect and a server retarget delete
    // the row, so a reconnect starts unshared.
    shared: boolean().notNull().default(false),
    ...timestamps,
  },
  (table) => [
    unique().on(table.serverId, table.userId),
    index(`idx_mcp_credentials_user`).on(table.userId),
  ]
)

// The RFC 7591 dynamic-registration cache: one client per (issuer,
// redirect_uri, registration_endpoint), shared by every team and member of
// the instance (a CIMD provider needs no row — the client id IS our
// client.json URL). The registration endpoint is part of the key so a
// metadata document naming another endpoint for the same issuer can never
// reuse (or overwrite) a client registered elsewhere.
// `client_secret_ciphertext` only when the AS issued a confidential client
// (AAD-bound to issuer + registration endpoint).
export const mcpOauthClients = pgTable(
  `mcp_oauth_clients`,
  {
    id: uuidPk(),
    issuer: text().notNull(),
    redirectUri: text(`redirect_uri`).notNull(),
    registrationEndpoint: text(`registration_endpoint`).notNull(),
    clientId: text(`client_id`).notNull(),
    clientSecretCiphertext: text(`client_secret_ciphertext`),
    tokenEndpointAuthMethod: varchar(`token_endpoint_auth_method`, {
      length: 32,
    })
      .notNull()
      .default(`none`),
    createdAt: timestamp(`created_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [
    unique(`mcp_oauth_clients_cache_key`).on(
      table.issuer,
      table.redirectUri,
      table.registrationEndpoint
    ),
  ]
)

// One member's MCP OAuth sign-in in flight. Minted by `mcpServers.connect`
// (state = 32 random bytes, base64url; 10-minute TTL, single use) with the
// PKCE verifier ENCRYPTED; the anonymous hosted callback matches `state`,
// exchanges the code SERVER-side and upserts `mcp_credentials`.
// `return_to` = a same-origin relative path the callback 302s to.
// `status`: pending → done | failed.
export const mcpOauthFlows = pgTable(
  `mcp_oauth_flows`,
  {
    id: uuidPk(),
    state: varchar({ length: 64 }).notNull().unique(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    serverId: uuid(`server_id`)
      .notNull()
      .references(() => mcpServers.id, { onDelete: `cascade` }),
    codeVerifierCiphertext: text(`code_verifier_ciphertext`).notNull(),
    clientId: text(`client_id`).notNull(),
    issuer: text().notNull(),
    tokenEndpoint: text(`token_endpoint`).notNull(),
    resource: text().notNull(),
    redirectUri: text(`redirect_uri`).notNull(),
    returnTo: text(`return_to`),
    status: varchar({ length: 16 }).notNull().default(`pending`),
    error: text(),
    createdAt: timestamp(`created_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
    completedAt: timestamp(`completed_at`, { withTimezone: true }),
  },
  (table) => [index(`idx_mcp_oauth_flows_user`).on(table.userId)]
)

// LEGACY read shim (NEW-2): actions became `prompts` (migration 0182 copied
// name, icon and body over and emptied this table). The table and its
// shape stay, EMPTY and never written, for the iOS store build and the
// ≤0.14.74 devices that still request the `actions` shape; NEW-8 drops it.
export const actions = pgTable(
  `actions`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    // EXP-273: the action's display glyph — a curated icon name from the
    // shared registry (same set as boards.icon). NULL = clients fall back to
    // the generic action glyph. Varchar rather than the pg enum so growing the
    // registry never needs a migration; the router validates against the
    // contract.
    icon: varchar({ length: 64 }),
    // The markdown prompt; ≤64KB enforced by the router zod.
    body: text().notNull(),
    // EXP-257: typed input schema — members fill the values in the run dialog
    // and the launcher injects them into the prompt. '[]' = no inputs.
    inputs: jsonb()
      .$type<ActionInputDef[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    // EXP-825: the hint the composer's text field shows while this action is
    // picked — what the requester should type now that free-text inputs are
    // gone (their placeholder/label seeded it). NULL = the generic hint.
    promptPlaceholder: varchar(`prompt_placeholder`, { length: 200 }),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.name),
    index(`idx_actions_team`).on(table.teamId),
  ]
)

// Per-user notification delivery prefs (SERVER-ONLY). Missing row = all
// defaults (email on, daily digest). Email is a free delivery channel, never a
// notification type and never plan-gated.
export const userNotificationPrefs = pgTable(`user_notification_prefs`, {
  userId: text(`user_id`)
    .primaryKey()
    .references(() => users.id, { onDelete: `cascade` }),
  emailEnabled: boolean(`email_enabled`).notNull().default(true),
  // Per-type opt-outs; a type absent from the map defaults to on. Keys are
  // notification_type values (issue_assigned, issue_comment, …).
  typePrefs: jsonb(`type_prefs`)
    .$type<Partial<Record<NotificationType, boolean>>>()
    .notNull()
    .default(sql`'{}'::jsonb`),
  // off (hourly) | daily — documented varchar (server-only logic, no native
  // picker). Defaults to the quieter daily digest.
  digest: varchar({ length: 16 }).notNull().default(`daily`),
  // Local hour (0–23, full hours only) the DAILY digest goes out at, read in
  // the user's `users.timezone`. Ignored by the legacy hourly (`off`) cadence.
  digestHour: integer(`digest_hour`).notNull().default(8),
  // EXP-801: may OTHER members' agents send this user an `agent_message`
  // over MCP? False blocks the send outright (no inbox row, no push) — unlike
  // type_prefs, which only mutes delivery. The user's own agents always pass.
  allowAgentMessages: boolean(`allow_agent_messages`).notNull().default(true),
  // Stable per-user secret embedded in one-click List-Unsubscribe links.
  unsubscribeToken: varchar(`unsubscribe_token`, { length: 64 })
    .notNull()
    .unique(),
  ...timestamps,
})

// NEW-67 (voice plan v3): the member's ONE voice settings row
// (`fixtures/voice.json`), SERVER-ONLY like the notification prefs: read
// with `voice.get`, written with `voice.update`, handed to the member's
// devices on every heartbeat. Missing row = the fixture's `defaults`.
// Replaces the per-device `launch_defaults.voice` of NEW-53 (migrated).
export const userVoiceSettings = pgTable(`user_voice_settings`, {
  userId: text(`user_id`)
    .primaryKey()
    .references(() => users.id, { onDelete: `cascade` }),
  // A `devices.device_id` of the member's own desktop; null = none. Not a
  // FK: device rows are keyed by their uuid `id`, and a removed device
  // simply stops being found (the page reads None).
  voiceDeviceId: varchar(`voice_device_id`, { length: 128 }),
  agentId: uuid(`agent_id`).references(() => agents.id, { onDelete: `set null` }),
  // S-3: LEGACY SHIMS, read by nobody. Speak-back is always on (`voice.get`
  // answers speakReplies true) and push to talk is on while a key is bound
  // (pushToTalk = pttShortcut !== ''); kept for old clients' writes.
  speakReplies: boolean(`speak_replies`).notNull().default(false),
  pushToTalk: boolean(`push_to_talk`).notNull().default(false),
  // S-3: the push-to-talk binding in `voice.json` `shortcutGrammar`'s
  // canonical text; empty = no key bound = push to talk off.
  pttShortcut: varchar(`ptt_shortcut`, { length: 32 }).notNull().default(``),
  wakeWord: boolean(`wake_word`).notNull().default(false),
  // Empty = `Hey {agent}`.
  wakePhrase: varchar(`wake_phrase`, { length: 64 }).notNull().default(``),
  // #NEW-73: one of `voice.json` `languages` values (the spoken replies).
  language: varchar(`language`, { length: 8 }).notNull().default(`en`),
  // A reference WAV's path ON the voice device (never uploaded) and what the
  // clip says; empty = the built-in voice.
  voiceFile: varchar(`voice_file`, { length: 512 }).notNull().default(``),
  voiceTranscript: varchar(`voice_transcript`, { length: 500 }).notNull().default(``),
  // #NEW-57 (meeting.json): which desktop audio a meeting transcribes beside
  // the microphone (`desktopAudioValues`), the apps `include` picks /
  // `exclude` leaves out (<= `limits.apps` {id, name}), and the running
  // meeting (null = none; written only by voice.startMeeting / stopMeeting).
  desktopAudio: varchar(`desktop_audio`, { length: 16 }).notNull().default(`off`),
  desktopAudioApps: jsonb(`desktop_audio_apps`).$type<{ id: string; name: string }[]>().notNull().default(sql`'[]'::jsonb`),
  meetingStartedAt: timestamp(`meeting_started_at`, { withTimezone: true }),
  ...timestamps,
}, (table) => [
  check(`user_voice_settings_language_check`, sql`${table.language} IN ('en', 'de')`),
  check(
    `user_voice_settings_desktop_audio_check`,
    sql`${table.desktopAudio} IN ('off', 'all', 'include', 'exclude')`
  ),
])

// Email audit ledger (SERVER-ONLY). One row per outbound app email — hourly
// digests and external widget-reporter mail (null userId). Per-notification email idempotency does NOT live here: it is the
// notifications.emailed_at claim the digest sweep stamps before sending.
export const emailDeliveries = pgTable(
  `email_deliveries`,
  {
    id: uuidPk(),
    // Nullable: external widget reporters have no users row.
    userId: text(`user_id`).references(() => users.id, { onDelete: `cascade` }),
    toEmail: varchar(`to_email`, { length: 320 }).notNull(),
    // NULL on rows written before the column existed and on rows whose send
    // never produced a result (queued-only digest rows, invite-cap refusals).
    subject: text(),
    // Legacy: the pre-digest per-event pipeline wrote one delivery per
    // notification row. No current path sets it (a digest email covers many
    // notifications; the widget-reporter paths have none) — kept for old
    // rows' audit trail.
    notificationId: uuid(`notification_id`).references(() => notifications.id, {
      onDelete: `set null`,
    }),
    issueId: uuid(`issue_id`).references(() => issues.id, {
      onDelete: `set null`,
    }),
    // digest|reporter_reply|reporter_confirmation|widget_resolution
    // |team_invite|password_reset|email_verification|contact — documented
    // varchar (legacy rows: notification, support_reply,
    // support_confirmation).
    kind: varchar({ length: 32 }).notNull(),
    // queued|sent|failed|suppressed|bounced|complained — documented varchar
    // (suppressed = the send-time application-side suppression check refused
    // the address; bounced/complained are stamped post-send by the SES
    // feedback webhook).
    status: varchar({ length: 16 }).notNull().default(`queued`),
    provider: varchar({ length: 16 }), // ses|smtp (legacy rows: resend)
    providerMessageId: text(`provider_message_id`),
    error: text(),
    sentAt: timestamp(`sent_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_email_deliveries_user`).on(table.userId),
    index(`idx_email_deliveries_issue`).on(table.issueId),
    // The admin console's global sent-mail list orders by created_at desc.
    index(`idx_email_deliveries_created`).on(table.createdAt),
    // Legacy pre-digest idempotency guard — inert on new rows (Postgres
    // treats NULLs as distinct, and notification_id is now always NULL).
    unique(`uniq_email_delivery_notification`).on(table.notificationId),
  ]
)

// Bounce/complaint feedback per recipient ADDRESS (SERVER-ONLY, admin
// console). One upserted row per address, fed by the SES→SNS feedback
// webhook (/api/webhooks/ses); `suppressed_at` records the admin putting the
// address on the SES account-level suppression list (EXP-227 — repeated
// sends to bouncing addresses damage sender reputation).
export const emailBounces = pgTable(`email_bounces`, {
  id: uuidPk(),
  email: varchar({ length: 320 }).notNull().unique(),
  // bounce|complaint — the LAST event's kind (documented varchar), EXCEPT
  // that a suppressing classification (complaint, or bounce_type Permanent)
  // is sticky: the webhook upsert keeps it when a later Transient/
  // non-suppressing event arrives out of order (REV-43).
  kind: varchar({ length: 16 }).notNull(),
  // SES bounce classification of the last event (e.g. Permanent/General;
  // sticky once Permanent, see `kind`); complaints carry the feedback type
  // in bounceSubType.
  bounceType: varchar(`bounce_type`, { length: 32 }),
  bounceSubType: varchar(`bounce_sub_type`, { length: 64 }),
  diagnostic: text(),
  eventCount: integer(`event_count`).notNull().default(1),
  lastEventAt: timestamp(`last_event_at`, { withTimezone: true }).notNull(),
  suppressedAt: timestamp(`suppressed_at`, { withTimezone: true }),
  ...timestamps,
})

// First-party conversion-funnel event log (SERVER-ONLY, admin console;
// EXP-362). Append-only — no updated_at on purpose. Cookieless: anonymous_id
// is a daily-rotating salted hash of ip+ua computed server-side (nothing is
// ever stored on the visitor's device), so it links events within one UTC day
// and nothing across days.
export const conversionEvents = pgTable(
  `conversion_events`,
  {
    id: uuidPk(),
    // set null (not cascade): the funnel history survives account deletion.
    userId: text(`user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    anonymousId: varchar(`anonymous_id`, { length: 64 }),
    // landing|return_visit|signup|onboarding_completed|team_created|
    // invite_sent|invite_accepted|first_issue_created|checkout_started|
    // subscription_first_active|seats_updated|plan_changed|cancel_scheduled|
    // subscription_resumed|subscription_canceled — documented varchar (typed
    // union in lib/conversion/events.ts), not a pg enum, so new names never
    // need an ALTER TYPE migration.
    name: varchar({ length: 64 }).notNull(),
    properties: jsonb().$type<Record<string, unknown>>(),
    createdAt: timestamp(`created_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [
    index(`idx_conversion_events_name_created`).on(table.name, table.createdAt),
    index(`idx_conversion_events_user`).on(table.userId),
    index(`idx_conversion_events_anon`).on(table.anonymousId),
    // The idempotency mechanism: writers always insert with ON CONFLICT DO
    // NOTHING, and these partial uniques decide what "once" means. Landing
    // dedupes to once per visitor per day because the anonymous id itself
    // rotates daily.
    uniqueIndex(`uniq_conversion_events_once_per_user`)
      .on(table.userId, table.name)
      .where(sql`name in ('signup', 'first_issue_created')`),
    uniqueIndex(`uniq_conversion_events_once_per_sub`)
      .on(table.name, sql`(properties->>'creemSubscriptionId')`)
      .where(
        sql`name in ('subscription_first_active', 'subscription_canceled')`
      ),
    uniqueIndex(`uniq_conversion_events_landing_daily`)
      .on(table.name, table.anonymousId)
      .where(sql`name = 'landing'`),
    // return_visit dedupes to once per signed-in user per UTC day; the day
    // string rides properties (writer: lib/conversion/capture.ts) because a
    // created_at date expression would need an AT TIME ZONE cast here.
    uniqueIndex(`uniq_conversion_events_return_visit_daily`)
      .on(table.userId, sql`(properties->>'day')`)
      .where(sql`name = 'return_visit'`),
  ]
)

// EXP-759: durable per-user client-platform ledger (SERVER-ONLY, admin
// console; never synced — no shape proxy). Touched by lib/auth/resolve-bearer.ts
// on every authenticated API request (in-process throttled): natives identify
// themselves through the `x-client-version` header, a cookie session is the
// web app. `platform` = web|ios|android|desktop|cli — a documented varchar
// (the typed union lives in lib/client-platforms.ts), not a pg enum. Sessions
// expire and devices rows get re-registered, so this is the ONE durable
// "which clients has this user used" record.
export const userClientPlatforms = pgTable(
  `user_client_platforms`,
  {
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    platform: varchar({ length: 16 }).notNull(),
    firstSeenAt: timestamp(`first_seen_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
    lastSeenAt: timestamp(`last_seen_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
    // Marketing version from x-client-version (`0.14.24`); NULL for web.
    lastVersion: varchar(`last_version`, { length: 32 }),
  },
  (table) => [
    primaryKey({ columns: [table.userId, table.platform] }),
    index(`idx_user_client_platforms_platform`).on(table.platform),
  ]
)

// Embeddable feedback-widget configs (server-only, NOT Electric-synced; read
// via the `widgets` tRPC router). One row = one paste-in snippet: a public
// key scoped to a destination team+board, plus the domain allowlist
// that gates cross-origin submissions.
export const widgetConfigs = pgTable(
  `widget_configs`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Where every submission lands (SLOP-4: ONE path, a submission IS an
    // issue, so the board is REQUIRED again). Cascade: a board delete takes
    // its widgets with it — a widget cannot exist without a target.
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    // `expw_` + 32 base62 chars. Public by design (it ships inside the host
    // page's snippet); the domain allowlist + rate limiting are the controls,
    // so it is stored in plaintext for direct lookup.
    publicKey: varchar(`public_key`, { length: 64 }).notNull().unique(),
    // Hostname[:port] patterns; `*.example.com` matches subdomains only.
    // Must be non-empty to serve — an empty list blocks the key at serve
    // time (EXP-209 removed allow-all); create/update require ≥1 domain.
    allowedDomains: jsonb(`allowed_domains`)
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    enabled: boolean().notNull().default(true),
    // Appearance/behavior overrides served to the widget loader. Validated
    // by formConfigSchema (apps/web lib/trpc/widgets.ts) on write and
    // re-sanitized field-by-field on read (lib/widget/service.ts):
    // { buttonLabel?, accentColor?, launcher?, emailRequired?,
    //   collectEmail?, collectName?, nameRequired?, customFields?,
    //   labelIds?, theme? }. (SLOP-4 dropped `modes`: one form.)
    formConfig: jsonb(`form_config`).$type<Record<string, unknown>>(),
    createdByUserId: text(`created_by_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    ...timestamps,
  },
  (table) => [index(`idx_widget_configs_team`).on(table.teamId)]
)

// One row per widget submission (server-only, NOT synced): the structured
// reporter contact + page/env context that must survive description edits.
// SLOP-4: every submission IS an issue (`issue_id` NOT NULL); the row is the
// issue's "Reported via widget" card and the reporter's identity for the
// magic-link conversation (`reporter_email` = the reply channel).
export const widgetSubmissions = pgTable(
  `widget_submissions`,
  {
    id: uuidPk(),
    // `set null` so deleting a config keeps reporter contact info on issues.
    widgetConfigId: uuid(`widget_config_id`).references(
      () => widgetConfigs.id,
      { onDelete: `set null` }
    ),
    issueId: uuid(`issue_id`)
      .notNull()
      .unique()
      .references(() => issues.id, { onDelete: `cascade` }),
    reporterEmail: varchar(`reporter_email`, { length: 320 }),
    reporterName: varchar(`reporter_name`, { length: 255 }),
    // Host-app user id passed via identify(); opaque to us.
    reporterExternalId: varchar(`reporter_external_id`, { length: 255 }),
    pageUrl: text(`page_url`),
    userAgent: text(`user_agent`),
    viewportWidth: integer(`viewport_width`),
    viewportHeight: integer(`viewport_height`),
    screenWidth: integer(`screen_width`),
    screenHeight: integer(`screen_height`),
    devicePixelRatio: doublePrecision(`device_pixel_ratio`),
    customData: jsonb(`custom_data`).$type<Record<string, unknown>>(),
    // Set-once when the reporter's one-way resolution email was sent (issue
    // closed). Never cleared on reopen — no re-notify on status churn.
    resolvedNotifiedAt: timestamp(`resolved_notified_at`, {
      withTimezone: true,
    }),
    // SLOP-4: when the reporter last opened their magic-link page (stamped
    // by /api/support/thread, best-effort) — the card's read receipt.
    lastReporterSeenAt: timestamp(`last_reporter_seen_at`, {
      withTimezone: true,
    }),
    ...timestamps,
  },
  (table) => [index(`idx_widget_submissions_config`).on(table.widgetConfigId)]
)

// What an OAuth-authenticated MCP client may touch (SERVER-ONLY, written by
// the /auth/consent page). One row per (user, oauth client); re-consenting
// replaces the selection. `teamIds` grants whole teams (including
// boards created later); `boardIds` grants individual boards. A token
// whose (user, client) pair has NO row gets no access — the holder must
// re-authenticate through the consent page. Session-cookie and UNSCOPED
// personal api-key access to /api/mcp is never grant-scoped (the user's own
// credentials keep full membership access); a personal key minted with the
// same team/board selection (FEED-76) carries it in `apikeys.metadata.scope`
// and is confined the same way, on /api/mcp only.
export const mcpGrants = pgTable(
  `mcp_grants`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    clientId: text(`client_id`)
      .notNull()
      .references(() => oauthApplications.clientId, { onDelete: `cascade` }),
    allTeams: boolean(`all_teams`).notNull().default(false),
    teamIds: jsonb(`team_ids`)
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    boardIds: jsonb(`board_ids`)
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    ...timestamps,
  },
  (table) => [unique().on(table.userId, table.clientId)]
)

// EXP-630: tracker imports (Linear today; any adapter that produces an
// ImportBundle tomorrow). SERVER-ONLY, tRPC-managed — never an Electric
// shape. One row per wizard run: the pasted credential lives here ONLY while
// the job is live (nulled by the same UPDATE that reaches a terminal state
// and by the worker's 24h sweep), `payload` holds the adapter's snapshot (or
// the raw bundle for `source='bundle'`), `preview`/`plan`/`progress`/`counts`
// are the wizard's read model. `status` is a documented varchar (draft |
// previewing | ready | running | completed | failed | cancelled) — zod-typed
// in apps/web/src/lib/import/bundle.ts, no pg enum (nothing syncs it).
// `claim_token` fences the worker: every write while running carries it, so a
// replica that lost its claim (stale heartbeat, reclaimed elsewhere) aborts
// instead of writing over the new owner.
export const importJobs = pgTable(
  `import_jobs`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    createdByUserId: text(`created_by_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    source: varchar({ length: 32 }).notNull(),
    status: varchar({ length: 16 }).notNull().default(`draft`),
    credential: text(),
    payload: jsonb(),
    preview: jsonb(),
    plan: jsonb(),
    progress: jsonb(),
    counts: jsonb(),
    error: text(),
    claimToken: uuid(`claim_token`),
    claimedAt: timestamp(`claimed_at`, { withTimezone: true }),
    startedAt: timestamp(`started_at`, { withTimezone: true }),
    finishedAt: timestamp(`finished_at`, { withTimezone: true }),
    // EXP-1076: the owner cleared the Recent imports list. Hides the row from
    // `imports.list` WITHOUT touching `import_entity_map` — the re-import
    // idempotency key must outlive the card.
    dismissedAt: timestamp(`dismissed_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_import_jobs_team`).on(table.teamId, table.createdAt),
    index(`idx_import_jobs_live`)
      .on(table.status)
      .where(sql`status in ('previewing', 'ready', 'running')`),
  ]
)

// EXP-630: external id → local id, one row per imported entity. The unique
// index IS the idempotency key: a crashed job resumes past what it already
// wrote, a re-run of the same source creates nothing new, and a later
// incremental re-sync would key on it. `external_ref` keeps the human handle
// (e.g. `MET-1092`) for boards that had to renumber.
export const importEntityMap = pgTable(
  `import_entity_map`,
  {
    id: uuidPk(),
    jobId: uuid(`job_id`)
      .notNull()
      .references(() => importJobs.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    source: varchar({ length: 32 }).notNull(),
    // board | status | label | user | issue | comment | attachment
    externalKind: varchar(`external_kind`, { length: 16 }).notNull(),
    externalId: text(`external_id`).notNull(),
    externalRef: text(`external_ref`),
    localId: text(`local_id`).notNull(),
    createdAt: timestamp(`created_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [
    uniqueIndex(`uniq_import_entity_map_external`).on(
      table.teamId,
      table.source,
      table.externalKind,
      table.externalId
    ),
    index(`idx_import_entity_map_job`).on(table.jobId),
  ]
)

// ---------------------------------------------------------------------------
// Zod schemas
// ---------------------------------------------------------------------------

export const selectTeamSchema = createSelectSchema(teams)
export const createTeamSchema = createInsertSchema(teams).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectTeamMemberSchema = createSelectSchema(teamMembers, {
  role: teamRoleSchema,
})
export const selectTeamInviteSchema = createSelectSchema(teamInvites, {
  role: teamRoleSchema,
})
export const selectBoardSchema = createSelectSchema(boards)
export const createBoardSchema = createInsertSchema(boards).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectIssueSchema = createSelectSchema(issues, {
  description: issueDescriptionSchema.nullable(),
  priority: issuePrioritySchema,
  status: issueStatusSchema,
  source: issueSourceSchema,
})
export const createIssueSchema = createInsertSchema(issues).omit({
  id: true,
  number: true,
  identifier: true,
  // Server-derived (populate_issue_board_context) — never client input.
  teamId: true,
  boardDeletedAt: true,
  boardArchivedAt: true,
  createdAt: true,
  updatedAt: true,
})

export const selectLabelSchema = createSelectSchema(labels)
export const createLabelSchema = createInsertSchema(labels).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

// Named *Row* to keep clear of the IssueStatus ENUM type in domain.ts.
export const selectIssueStatusRowSchema = createSelectSchema(issueStatuses, {
  category: issueStatusCategorySchema,
  builtinKey: issueStatusSchema.nullable(),
})

export const selectIssueLabelSchema = createSelectSchema(issueLabels)

export const selectIssueRelationSchema = createSelectSchema(issueRelations, {
  type: issueRelationTypeSchema,
  source: issueRelationSourceSchema,
})

export const selectUserSchema = createSelectSchema(users)

export const selectCommentSchema = createSelectSchema(comments, {
  body: commentBodyWithAttachmentsSchema,
})
export const createCommentSchema = createInsertSchema(comments).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectAttachmentSchema = createSelectSchema(attachments)

export const selectNotificationSchema = createSelectSchema(notifications)

export const selectPinSchema = createSelectSchema(pins, {
  kind: pinKindSchema,
})

export const selectIssueDraftSchema = createSelectSchema(issueDrafts, {
  priority: issuePrioritySchema,
})

export const selectIssueSubscriberSchema = createSelectSchema(
  issueSubscribers,
  {
    source: subscriberSourceSchema,
  }
)

export const selectIssueEventSchema = createSelectSchema(issueEvents, {
  type: issueEventTypeSchema,
})


export const selectRepositorySchema = createSelectSchema(repositories)

// EXP-1291: the synced projections of the session tree. Tolerant jsonb reads
// (a future card kind or trigger must never brick a collection).
export const selectSessionSchema = createSelectSchema(sessions, {
  kind: sessionKindSchema,
  status: sessionStatusSchema,
  guide: z.custom<GuideEntry[]>((value) => value === null || Array.isArray(value)),
  triggers: z.custom<AgentTrigger[]>((value) => Array.isArray(value)),
  actionInputs: z.record(z.string(), z.string()).nullable(),
  mcpServerIds: z.array(z.string()).nullable(),
})
export const selectSyncedSessionSchema = selectSessionSchema.omit({
  mcpServerIds: true,
  wakeOn: true,
  // EXP-1374: the stuck rule's clock, server-only.
  lastActivityAt: true,
})

// NEW-2: agents, events, prompts.
export const selectAgentSchema = createSelectSchema(agents, {
  triggers: z.custom<AgentTrigger[]>((value) => Array.isArray(value)),
  triggerState: z.custom<AgentTriggerState>((value) => value !== null && typeof value === `object`),
})
export const selectSyncedAgentSchema = selectAgentSchema.omit({
  instructions: true,
  triggerState: true,
})
export const selectEventSchema = createSelectSchema(events, {
  emittedBy: z.enum(eventEmitterValues),
  payload: z.custom<Record<string, unknown>>((value) => value !== null && typeof value === `object`),
})
export const selectPromptSchema = createSelectSchema(prompts)
// NEW-3
export const selectConversationSchema = createSelectSchema(conversations, {
  kind: conversationKindSchema,
})
export const selectConversationMemberSchema = createSelectSchema(conversationMembers)
export const selectSyncedConversationSchema = selectConversationSchema.omit({
  shared: true,
  ownerId: true,
})
export const selectSyncedPromptSchema = selectPromptSchema.omit({ body: true })
// VAPP-110: apps (the head package rides along) and their versions (the
// package stays server-side).
export const selectAppSchema = createSelectSchema(apps, {
  package: z.custom<AppPackageJson | null>((value) => value === null || (typeof value === `object` && !Array.isArray(value))),
})
export const selectAppVersionSchema = createSelectSchema(appVersions, {
  package: z.custom<AppPackageJson>((value) => typeof value === `object` && value !== null && !Array.isArray(value)),
})
export const selectSyncedAppVersionSchema = selectAppVersionSchema.omit({ package: true })
// VAPP-122: records (the app mirror stays behind the shape).
export const selectAppRecordSchema = createSelectSchema(appRecords, {
  data: z.custom<AppRecordData>((value) => typeof value === `object` && value !== null && !Array.isArray(value)),
})
export const selectSyncedAppRecordSchema = selectAppRecordSchema.omit({ appDeletedAt: true })
export const selectSessionIssueSchema = createSelectSchema(sessionIssues, {
  source: sessionIssueSourceSchema,
})
export const selectSyncedSessionIssueSchema = selectSessionIssueSchema.omit({
  boardId: true,
  boardDeletedAt: true,
  boardArchivedAt: true,
  shared: true,
  ownerId: true,
})
export const selectMessageSchema = createSelectSchema(messages, {
  authorKind: messageAuthorKindSchema,
  card: z.custom<MessageCard>((value) => value === null || typeof value === `object`).nullable(),
})
export const selectSyncedMessageSchema = selectMessageSchema.omit({
  shared: true,
  ownerId: true,
  // #NEW-57: server-only (the meeting prune), never synced.
  expiresAt: true,
})
export const selectSessionReadSchema = createSelectSchema(sessionReads)
// FEED-160: `github_updated_at` is server-only (outside the shape allowlist).
export const selectPullRequestSchema = createSelectSchema(pullRequests, {
  state: prStateSchema,
}).omit({ githubUpdatedAt: true })
export const selectPullRequestIssueSchema = createSelectSchema(pullRequestIssues)
export const selectSyncedPullRequestIssueSchema = selectPullRequestIssueSchema.omit({
  boardId: true,
  boardDeletedAt: true,
  boardArchivedAt: true,
})

export const selectMcpServerSchema = createSelectSchema(mcpServers, {
  transport: mcpTransportSchema,
  auth: mcpAuthSchema,
  headerNames: z.array(z.string()),
  args: z.array(z.string()),
  envNames: z.array(z.string()),
  scopes: z.array(z.string()),
})

export const selectActionSchema = createSelectSchema(actions, {
  inputs: actionInputsSchema,
})

// The shape-synced projection: the actions shape pins a columns allowlist
// that EXCLUDES `body` (the ≤64KB prompt never rides sync — fetched via
// tRPC `actions.get` on demand).
export const selectSyncedActionSchema = selectActionSchema.omit({ body: true })

export const selectWidgetConfigSchema = createSelectSchema(widgetConfigs, {
  allowedDomains: z.array(z.string()),
  formConfig: z.record(z.string(), z.unknown()).nullable(),
})

export const selectWidgetSubmissionSchema = createSelectSchema(
  widgetSubmissions,
  {
    customData: z.record(z.string(), z.unknown()).nullable(),
  }
)

export const selectDeviceSchema = createSelectSchema(devices, {
  agents: z.array(z.string()),
  caps: z.array(z.string()),
  unauthedAgents: z.array(z.string()),
  acpAgents: z.array(z.string()).nullable(),
  launchDefaults: deviceLaunchDefaultsSchema.nullable(),
  agentAccounts: deviceAgentAccountsSchema.nullable(),
  agentUsage: deviceAgentUsageSchema.nullable(),
  // EXP-1196: stored rows are already clamped server-side; the synced copy
  // is taken as-is (no transforms in a collection schema) — clients skip
  // vocabulary they cannot name.
  doctor: z
    .custom<DeviceDoctor>(
      (value) =>
        typeof value === `object` &&
        value !== null &&
        Array.isArray((value as { items?: unknown }).items)
    )
    .nullable(),
})

export const selectDeviceWorktreeSchema = createSelectSchema(deviceWorktrees, {
  agents: z.array(z.string()).nullable(),
})

// The shape-synced projection: the device-worktrees shape pins a columns
// allowlist that EXCLUDES the trigger-maintained scoping mirrors (the where
// clause filters on them server-side).
export const selectSyncedDeviceWorktreeSchema = selectDeviceWorktreeSchema.omit(
  {
    userId: true,
    sharedTeamIds: true,
  }
)

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type Team = InferSelectModel<typeof teams>
export type TeamMember = InferSelectModel<typeof teamMembers>
export type TeamInvite = InferSelectModel<typeof teamInvites>
export type Board = InferSelectModel<typeof boards>
export type Issue = InferSelectModel<typeof issues>
export type Label = InferSelectModel<typeof labels>
export type IssueStatusRow = InferSelectModel<typeof issueStatuses>
export type IssueLabel = InferSelectModel<typeof issueLabels>
export type IssueRelation = InferSelectModel<typeof issueRelations>
export type Comment = InferSelectModel<typeof comments>
export type Attachment = InferSelectModel<typeof attachments>

export type User = InferSelectModel<typeof users>
export type Notification = InferSelectModel<typeof notifications>
export type Pin = InferSelectModel<typeof pins>
export type SessionRead = InferSelectModel<typeof sessionReads>
export type IssueDraft = InferSelectModel<typeof issueDrafts>
export type IssueSubscriber = InferSelectModel<typeof issueSubscribers>
export type IssueEvent = InferSelectModel<typeof issueEvents>
export type Session = InferSelectModel<typeof sessions>
export type SessionIssue = InferSelectModel<typeof sessionIssues>
export type Message = InferSelectModel<typeof messages>
// NEW-3
export type Conversation = InferSelectModel<typeof conversations>
export type ConversationMember = InferSelectModel<typeof conversationMembers>
export type PullRequest = InferSelectModel<typeof pullRequests>
export type PullRequestIssue = InferSelectModel<typeof pullRequestIssues>
export type Repository = InferSelectModel<typeof repositories>
export type McpServer = InferSelectModel<typeof mcpServers>
export type McpCredential = InferSelectModel<typeof mcpCredentials>
export type McpOauthClient = InferSelectModel<typeof mcpOauthClients>
export type McpOauthFlow = InferSelectModel<typeof mcpOauthFlows>
export type DeviceAgentProfile = z.infer<typeof deviceAgentProfileSchema>
export type Action = InferSelectModel<typeof actions>
export type SyncedAction = Omit<Action, `body`>
export type Agent = InferSelectModel<typeof agents>
export type SyncedAgent = Omit<Agent, `instructions` | `triggerState`>
export type Event = InferSelectModel<typeof events>
export type Prompt = InferSelectModel<typeof prompts>
export type SyncedPrompt = Omit<Prompt, `body`>
// VAPP-110
export type App = InferSelectModel<typeof apps>
export type AppVersion = InferSelectModel<typeof appVersions>
export type SyncedAppVersion = Omit<AppVersion, `package`>
export type AppRecord = InferSelectModel<typeof appRecords>
export type SyncedAppRecord = Omit<AppRecord, `appDeletedAt`>
export type UserNotificationPrefs = InferSelectModel<
  typeof userNotificationPrefs
>
export type EmailDelivery = InferSelectModel<typeof emailDeliveries>
export type ConversionEvent = InferSelectModel<typeof conversionEvents>
export type UserClientPlatformRow = InferSelectModel<typeof userClientPlatforms>
export type WidgetConfig = InferSelectModel<typeof widgetConfigs>
export type WidgetSubmission = InferSelectModel<typeof widgetSubmissions>
export type McpGrant = InferSelectModel<typeof mcpGrants>
export type Device = InferSelectModel<typeof devices>
export type DeviceWorktree = InferSelectModel<typeof deviceWorktrees>
export type SyncedDeviceWorktree = Omit<
  DeviceWorktree,
  `userId` | `sharedTeamIds`
>
export type DeviceCommand = InferSelectModel<typeof deviceCommands>

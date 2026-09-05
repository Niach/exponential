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
  type AutomationTrigger,
  codingSessionStatusSchema,
  commentBodyWithAttachmentsSchema,
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
  type NotificationType,
  notificationTypeValues,
  prStateSchema,
  prStateValues,
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

// The PG type keeps the orphan `merged` label (EXP-540 migrated every row to
// `ended`): dropping an enum VALUE needs a full type recreate, and no writer
// or reader can produce it any more. Deliberately NOT derived from
// codingSessionStatusValues — that list is the app vocabulary.
export const codingSessionStatusEnum = pgEnum(`coding_session_status`, [
  `running`,
  `in_review`,
  `merged`,
  `ended`,
])

export const issueEventTypeEnum = pgEnum(
  `issue_event_type`,
  issueEventTypeValues
)

export const subscriberSourceEnum = pgEnum(
  `subscriber_source`,
  subscriberSourceValues
)

export const issueRelationTypeEnum = pgEnum(
  `issue_relation_type`,
  issueRelationTypeValues
)

export const issueRelationSourceEnum = pgEnum(
  `issue_relation_source`,
  issueRelationSourceValues
)

// EXP-741: who posted a comment (a person, or an agent over MCP).
export const commentSourceEnum = pgEnum(`comment_source`, commentSourceValues)

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
  // Team-level helpdesk switch (EXP-180 — replaced the per-board flag;
  // Pro-gated on cloud via assertCanUseHelpdesk on enable and per submission).
  // Synced so every client can gate its Support-inbox menu entry; the
  // conversation tables themselves stay server-only.
  helpdeskEnabled: boolean(`helpdesk_enabled`).notNull().default(false),
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
    acceptedAt: timestamp(`accepted_at`, { withTimezone: true }),
    expiresAt: timestamp(`expires_at`, { withTimezone: true }).notNull(),
    ...timestamps,
  },
  (table) => [index(`idx_team_invites_team`).on(table.teamId)]
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
    dueDate: date(`due_date`),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    completedAt: timestamp(`completed_at`, { withTimezone: true }),
    // Duplicate resolution: this issue is a duplicate of the canonical issue.
    // 1:1 (no relation graph); pairs with status='duplicate'.
    duplicateOfId: uuid(`duplicate_of_id`).references(
      (): AnyPgColumn => issues.id,
      { onDelete: `set null` }
    ),
    // PR linkage (one issue = one PR = one branch/worktree). Kept on the
    // issue row (PR is 1:1 with the issue) and synced to every client so the
    // diff view + PR badge work without parsing comment bodies. Written by the
    // MCP `open_pr` tool and the merge webhook/cron. All nullable (no PR until
    // one is opened).
    prUrl: text(`pr_url`),
    prNumber: integer(`pr_number`),
    prState: prStateEnum(`pr_state`),
    branch: text(`branch`),
    prMergedAt: timestamp(`pr_merged_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_issues_board_status`).on(table.boardId, table.status),
    index(`idx_issues_team`).on(table.teamId),
    index(`idx_issues_assignee`).on(table.assigneeId),
    index(`idx_issues_creator`).on(table.creatorId),
    index(`idx_issues_due_date`).on(table.dueDate),
    // PR→issue resolution is exact-pr_url match (batch PRs link many issues to
    // one URL): the webhook, merge/close batch-sibling lookups, and the
    // self-hosted poller all filter on it. Partial — most issues have no PR.
    index(`idx_issues_pr_url`)
      .on(table.prUrl)
      .where(sql`pr_url IS NOT NULL`),
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
    // byte-identical to the query's tsvector expression in trpc/issues.ts —
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
    authorId: text(`author_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // EXP-741: the top-level comment this one replies to — ONE level deep
    // (`comments.create` flattens a reply-to-a-reply onto the root), same
    // issue, cascade-deleted with its parent. NULL = a top-level comment.
    parentId: uuid(`parent_id`).references((): AnyPgColumn => comments.id, {
      onDelete: `cascade`,
    }),
    // EXP-741: `mcp` when an agent posted it over MCP (the card header shows
    // "via MCP"); stamped server-side from the MCP context, never by input.
    source: commentSourceEnum().notNull().default(`user`),
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
    // Serves issues.search's comment-body FTS branch (REV-14). Same
    // byte-identical-expression contract as idx_issues_fts.
    index(`idx_comments_body_fts`).using(
      `gin`,
      sql`to_tsvector('english', ${table.body})`
    ),
  ]
)

// The live "coding now" record — one row per interactive desktop coding
// session (one terminal tab + one agent CLI child — claude/codex/pi,
// EXP-201). SYNCED as an Electric shape
// so every coordination client shows the badge + Watch/Steer button. No
// plan/approval state, no run history, no slot pool — PR outcome lives on
// `issues` (prUrl/prNumber/prState/branch). Three session subjects: issue-
// scoped (`issue_id` set — one worktree, one issue), batch-scoped (`issue_id`
// and `board_id` NULL, `team_id` written directly — the desktop multi-issue
// batch run), and action-scoped (EXP-253: like batch but with `action_id` +
// the `action_name` display snapshot — actions are server-only, never synced,
// so clients label rows off the snapshot). Enforced by the tRPC writer
// (exactly one of issueId/teamId/actionId in the start input). `team_id` is
// denormalized from issue→board by trigger for issue rows (the populate
// triggers no-op when issue_id IS NULL).
export const codingSessions = pgTable(
  `coding_sessions`,
  {
    id: uuidPk(),
    // Nullable: NULL for batch (multi-issue) orchestrator sessions.
    issueId: uuid(`issue_id`).references(() => issues.id, {
      onDelete: `cascade`,
    }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Denormalized from issue→board (populate_issue_child_board_id).
    // Nullable: a batch-scoped session spans boards and carries no board
    // identity.
    boardId: uuid(`board_id`).references(() => boards.id, {
      onDelete: `cascade`,
    }),
    // Mirrors of the parent board's deleted_at / archived_at
    // (trigger-maintained, REV2-5 + EXP-500) — the shape's static trash and
    // archive predicates; both stay NULL on batch rows, which therefore
    // always sync. Server-only, shape-excluded.
    boardDeletedAt: timestamp(`board_deleted_at`, { withTimezone: true }),
    boardArchivedAt: timestamp(`board_archived_at`, { withTimezone: true }),
    // Action-scoped sessions (EXP-253): the action being run (SET NULL — a
    // deleted action degrades the row to batch-shaped) plus a display-name
    // snapshot (actions are server-only, never synced — clients can't join).
    actionId: uuid(`action_id`).references(() => actions.id, {
      onDelete: `set null`,
    }),
    actionName: varchar(`action_name`, { length: 255 }),
    // EXP-530: why the session started — 'schedule' | 'event' | 'agent'
    // (documented varchar, startedReasonValues in domain.ts; NULL = a person
    // started it). Set only by codingSessions.start: schedule/event when a
    // device's automation host fires an action trigger, and 'agent'
    // (EXP-679) when another coding session started this one through
    // `exponential_sessions_start` — equally unattended, so its close-out
    // ends it. Powers the "Automated" badge + Automations run history on
    // every client (synced via the shape).
    startedReason: varchar(`started_reason`, { length: 16 }),
    // EXP-583: the automation that fired this run (SET NULL — history outlives
    // a deleted automation; NULL for every human start). Powers per-automation
    // "last run" on every client (synced via the shape).
    automationId: uuid(`automation_id`).references(() => automations.id, {
      onDelete: `set null`,
    }),
    // EXP-679: the run that started this one via `exponential_sessions_start`
    // (stamped by the MCP tool from its session header). SERVER-ONLY —
    // exposed through tRPC / MCP `sessions_get`, NEVER in the shape
    // allowlist; history only, so SET NULL keeps the child when the parent
    // is purged.
    parentSessionId: uuid(`parent_session_id`).references(
      (): AnyPgColumn => codingSessions.id,
      { onDelete: `set null` }
    ),
    // The real user driving the session under their own auth — NOT a synthetic
    // agent identity. For a start on a teammate's shared server device
    // (EXP-432) this is the REQUESTER, so EXP-312 owner-only steering lets
    // them watch/steer/kill what they launched.
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // EXP-432, server-only — NEVER add to the shape allowlist. The device
    // owner's account hosting a teammate-started session on a shared server
    // device: their daemon executes the run under its own auth, so session
    // procedures (get/heartbeat/setNeedsInput/end, publisher tickets, the MCP
    // batch flip) accept host OR owner. NULL for every self-hosted start.
    // SET NULL, not cascade: the row belongs to the requester and survives
    // the host's account deletion.
    hostUserId: text(`host_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    // Human label of the host device ("Dennis's MacBook"), shown on the badge.
    // Since EXP-549 a SNAPSHOT of the devices row's `label` at start/heartbeat
    // time (server-resolved from `device_id`; the client-sent hostname is the
    // fallback) — clients prefer the live devices row (see `deviceId`).
    deviceLabel: varchar(`device_label`, { length: 255 }),
    // EXP-549/550: the steer deviceId of the machine hosting the session
    // (= `devices.device_id` of the caller's own row; no FK — devices rows are
    // per (user, deviceId) and removable). Synced, so clients join the live
    // devices row for the RENAMED label and its `last_seen_at` freshness (the
    // "paused — device offline" state). NULL on rows from pre-EXP-549 clients.
    deviceId: varchar(`device_id`, { length: 128 }),
    // EXP-484: the agent CLI running the session (documented varchar, values
    // = contract `codingAgent`, like `started_reason`). Written by the
    // launcher at start (and echoed on the heartbeat re-create); NULL on rows
    // from clients that predate it. Synced, so every client can show which
    // agent a run uses and pair it with the device's usage windows.
    agent: varchar({ length: 16 }),
    status: codingSessionStatusEnum().notNull().default(`running`),
    // EXP-545: the batch↔PR linkage. Stamped with the PR's head branch
    // (`exp/batch-<id8>`) when the MCP pr_open batch flip parks the row in
    // `in_review`, so clients tie a batch session's Merge shortcut to ITS
    // OWN PR instead of "the team's sole open batch PR" (which could target
    // a teammate's PR once the session's own PR closed unmerged). NULL on
    // issue-scoped sessions (the issue row carries the branch) and on batch
    // rows whose PR isn't open yet; action and chat rows carry their run
    // branch from the start (EXP-637) and the chore pr_open re-stamps it.
    branch: varchar(`branch`, { length: 255 }),
    // EXP-734: the pull request an issue-LESS run opened — the chore PR of an
    // action or chat run (`exponential_pr_open({ repositoryId, head })`,
    // EXP-626). Populated ONLY for PRs that link no issue; issue-scoped and
    // batch rows keep reading the issue row(s) (prUrl/prNumber/prState
    // there). Synced, so every client's Merge shortcut and Reviews queue key
    // on the run itself; `applySessionPrState` (pr-sync) keeps `pr_state` in
    // step on every merge/close/reopen path (merge helper, webhook, poller)
    // and ends the run on merge like every other path.
    prUrl: text(`pr_url`),
    prNumber: integer(`pr_number`),
    prState: prStateEnum(`pr_state`),
    // EXP-637 close-out (all synced): the agent's own account of the run,
    // written ONCE by the `exponential_sessions_end` MCP tool (the calling
    // session is identified by the launcher-injected `X-Exp-Session-Id`
    // header). `summary` is plain GFM text ≤ MAX_CODING_SESSION_SUMMARY; it
    // stays NULL on every other end path, and an already-ended row never has
    // it overwritten.
    summary: text(`summary`),
    // Who ended the run (codingSessionEndedByValues, documented varchar):
    // `agent` (sessions_end) · `user` (steer.killSession) · `client`
    // (codingSessions.end — agent exit, tab close, quit) · `merge` (the PR
    // merge paths) · `system` (the stale sweep, account deletion). NULL on
    // rows ended by pre-EXP-637 servers.
    endedBy: varchar(`ended_by`, { length: 16 }),
    // The ended run this one resumes (steer.startSession({ resumeSessionId })
    // or the desktop's Resume). SET NULL so purging an old run keeps the
    // resumed row; clients use it to match a resume they just requested.
    resumedFromId: uuid(`resumed_from_id`).references(
      (): AnyPgColumn => codingSessions.id,
      { onDelete: `set null` }
    ),
    // SERVER-ONLY (never in the shape allowlist, like host_user_id): the
    // session merged the PR it opened, via the MCP `exponential_pr_merge`
    // tool with its own session header. Every merge-driven end skips flagged
    // rows (EXP-637 decision 6) — a session that merges its own PR ends only
    // through `exponential_sessions_end` or its own exit, never through the
    // merge it just performed (webhook and poller spare it too, which is why
    // this is a durable column and not an in-memory claim).
    mergedOwnPr: boolean(`merged_own_pr`).notNull().default(false),
    // Desktop-written attention flag (EXP-214): the agent is parked on a
    // plan-approval or AskUserQuestion picker and waits for a human. Composes
    // with running/in_review (which stay server-owned) instead of being a
    // status of its own; cleared by the desktop when the picker resolves.
    needsInput: boolean(`needs_input`).notNull().default(false),
    // EXP-701: the device's pickup ack. The launching device creates this row
    // right before it spawns the agent, then its FIRST liveness heartbeat —
    // fired immediately after the spawn — stamps this (the server coalesces it
    // on every `codingSessions.heartbeat`, since any beat proves the run's
    // host is alive). A `running` row whose acked_at stays NULL for minutes
    // means the device died between creating the row and spawning the agent —
    // the signal orchestrators use to tell a dead start from a quiet run.
    // SERVER-ONLY (never in the shape allowlist, like merged_own_pr): exposed
    // through tRPC `codingSessions.get` and the MCP session tools.
    ackedAt: timestamp(`acked_at`, { withTimezone: true }),
    startedAt: timestamp(`started_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
    endedAt: timestamp(`ended_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_coding_sessions_issue`).on(table.issueId),
    index(`idx_coding_sessions_team`).on(table.teamId),
    index(`idx_coding_sessions_board`).on(table.boardId),
    index(`idx_coding_sessions_user`).on(table.userId),
    index(`idx_coding_sessions_action`).on(table.actionId),
  ]
)

export const attachments = pgTable(
  `attachments`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    // Denormalized from issue→board (populate_issue_child_board_id) so the
    // trash fan-out can target a board's child rows. Attachment byte reads
    // are member-only too (EXP-180).
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
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
    ...timestamps,
  },
  (table) => [
    index(`idx_attachments_issue`).on(table.issueId),
    index(`idx_attachments_team`).on(table.teamId),
    index(`idx_attachments_board`).on(table.boardId),
    // The comment_id SET NULL RI trigger fires on every comment delete.
    // Partial — most attachments aren't comment-embedded.
    index(`idx_attachments_comment`)
      .on(table.commentId)
      .where(sql`comment_id IS NOT NULL`),
  ]
)

// EXP-702: steer images. EVERY image attached to a steered message lands
// here — issue runs included, so steering screenshots never clutter the
// issue's Files section, and issue-less runs (chat/action/batch) get image
// upload at all. SERVER-ONLY, never synced: the natives pin `issue_id` NOT
// NULL on the synced attachments table, so these rows must never ride that
// shape.
// They are served by the SAME `/api/attachments/{id}` read route, keeping the
// load-bearing steer embed `![image](/api/attachments/{id})` (EXP-511) intact
// for every host and viewer. `session_id` is SET NULL (the staleness sweep
// deletes coding_sessions rows), and the orphan reclaim sweep
// (apps/web/src/lib/session-attachment-sweep.ts) deletes such rows plus their
// blobs after a 7-day grace window — otherwise unreachable bytes would count
// against the team's storage budget forever. Blob reclamation also keys on
// `team_id` at team/account deletion, exactly like issue attachments.
export const sessionAttachments = pgTable(
  `session_attachments`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    sessionId: uuid(`session_id`).references(() => codingSessions.id, {
      onDelete: `set null`,
    }),
    uploaderId: text(`uploader_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    filename: varchar({ length: 500 }).notNull(),
    contentType: varchar(`content_type`, { length: 255 }).notNull(),
    sizeBytes: bigint(`size_bytes`, { mode: `number` }).notNull(),
    storageKey: text(`storage_key`).notNull(),
    url: text().notNull(),
    // Intrinsic pixel dimensions, probed at upload time (nullable — probing
    // is best-effort, matching attachments).
    width: integer(),
    height: integer(),
    ...timestamps,
  },
  (table) => [
    index(`idx_session_attachments_team`).on(table.teamId),
    index(`idx_session_attachments_session`).on(table.sessionId),
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
  ultracode?: boolean
  planMode?: boolean
}
export interface DeviceLaunchDefaults {
  defaultAgent?: string
  agents?: Record<string, DeviceAgentLaunchDefaults>
}
// Every field is `.nullish()`, not `.optional()`: 0.14.10 native builds
// (EXP-495) serialized capability-masked toggles as explicit `null` and a
// strict schema 400'd the whole register — leaving the machine invisible
// with no self-heal path (the update request is consumed on register).
// `clampLaunchDefaults` strips the nulls; stored copies stay null-free.
// Plain `z.object` (strip mode) on purpose — never `.strict()`: an unknown
// key from a newer or older client is dropped, not a reason to 400 the
// register.
export const deviceLaunchDefaultsSchema = z.object({
  defaultAgent: z.string().min(1).max(32).nullish(),
  agents: z
    .record(
      z.string().min(1).max(32),
      z.object({
        model: z.string().max(64).nullish(),
        effort: z.string().max(64).nullish(),
        ultracode: z.boolean().nullish(),
        planMode: z.boolean().nullish(),
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
}
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
export const deviceAgentAccountsSchema = z.record(
  z.string(),
  z
    .object({
      signedIn: z.boolean().nullish(),
      email: z.string().max(320).nullish(),
      plan: z.string().max(64).nullish(),
      checkedAt: z.string().max(64).nullish(),
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
    // EXP-432: the ONE team this device is shared with (server-kind only,
    // router-enforced). Teammates of that team see the device in team-scoped
    // devices.list and may remote-start sessions on it; the resulting rows
    // are requester-owned with host_user_id = this row's owner. SET NULL on
    // team delete; NULL = private (the default).
    sharedTeamId: uuid(`shared_team_id`).references(() => teams.id, {
      onDelete: `set null`,
    }),
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
    // machine last probed it. NULL = never reported (older build) — clients
    // render no Agents section rather than claiming "signed out".
    agentAccounts: jsonb(`agent_accounts`).$type<DeviceAgentAccounts>(),
    // EXP-484: per-agent usage windows as the machine last fetched them,
    // plus the stamp of that write. The stamp moves every few minutes and is
    // NEVER a convergence trigger: the desktop watches
    // `launch_defaults_updated_at` only, or every usage refresh would nudge
    // the fleet.
    agentUsage: jsonb(`agent_usage`).$type<DeviceAgentUsageMap>(),
    agentUsageAt: timestamp(`agent_usage_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.userId, table.deviceId),
    index(`idx_devices_user`).on(table.userId),
    // Serves the per-team shared listing; partial — most devices are private.
    index(`idx_devices_shared_team`)
      .on(table.sharedTeamId)
      .where(sql`shared_team_id IS NOT NULL`),
  ]
)

// EXP-481: per-device worktree inventory, reported by the device
// (devices.reportWorktrees, full current set diff-upserted server-side) —
// powers resume offers, listing and prune even while the device is offline.
// Synced shape #18. `user_id` + `shared_team_id` are trigger-maintained
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
    sharedTeamId: uuid(`shared_team_id`).references(() => teams.id, {
      onDelete: `set null`,
    }),
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
    // Partial — most devices are private.
    index(`idx_device_worktrees_shared_team`)
      .on(table.sharedTeamId)
      .where(sql`shared_team_id IS NOT NULL`),
  ]
)

// EXP-481: owner→device work queue (SERVER-ONLY, never synced — a bilateral
// concern, not team product data). Created by the owner via
// devices.createCommand, delivered on the device's heartbeat (plus a relay
// check_in nudge for immediacy), completed via devices.completeCommand.
// Rows stay `pending` until completed — redelivery on a missed cycle is free
// idempotency. `kind` is a documented varchar: `worktree_remove` (payload
// {repoFullName, branch}) | `worktree_prune` (payload {}) | `agent_login`
// (EXP-484, payload {agent, switch: "true"|"false"} — the device runs the
// agent CLI's own login flow and completes the command EARLY, as soon as the
// sign-in URL is on screen, with the JSON progress in `result`).
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

// GitHub App installations (server-only, not synced). Mirrored from the setup
// redirect, the OAuth claim callback, and installation webhooks; token
// resolution itself is storage-free (the App JWT looks up a repo's installation
// on demand). Visibility is granted per team via githubInstallationLinks —
// an unlinked row is invisible to every picker.
export const githubInstallations = pgTable(`github_installations`, {
  id: uuidPk(),
  installationId: bigint(`installation_id`, { mode: `number` })
    .notNull()
    .unique(),
  accountLogin: text(`account_login`),
  accountType: varchar(`account_type`, { length: 20 }),
  // GitHub-side SUSPENSION marker (REV2-29). Suspension is REVERSIBLE — GitHub
  // keeps the installation, just refuses to mint tokens for it — so the
  // `suspend` webhook marks this column instead of deleting the row. Deleting
  // it CASCADE-dropped every team's claim link, and `unsuspend` re-inserted a
  // row with a FRESH uuid PK, so the links (which reference that uuid) were
  // unrecoverable by construction: a suspend→unsuspend cycle silently cost
  // every claiming team its coding/PR/token features until an owner re-ran the
  // connect flow by hand. Row deletion is now reserved for the terminal
  // `deleted` action. A marked installation is inert but recoverable —
  // discovery/connect refuse it, `unsuspend` (or the probe heal in
  // lib/trpc/integrations.ts) clears the mark and everything resumes.
  suspendedAt: timestamp(`suspended_at`, { withTimezone: true }),
  ...timestamps,
})

// Team ↔ GitHub App installation claims (SERVER-ONLY, never synced).
// A link means "this team may browse/connect this installation's repos".
// Created by the OAuth claim flow (or the install-page round-trip fallback) —
// both prove control of the GitHub account before linking. Many-to-many: one
// org install can serve several teams, one team can link several
// GitHub accounts. CASCADE on the installation FK: when the UNINSTALL webhook
// (`installation.deleted` — the only terminal action) deletes the
// github_installations row, its links vanish with it. A mere `suspend` must
// never take this path (REV2-29): it marks `suspended_at` instead, so an
// `unsuspend` restores every claim without a manual reconnect.
export const githubInstallationLinks = pgTable(
  `github_installation_links`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    githubInstallationId: uuid(`github_installation_id`)
      .notNull()
      .references(() => githubInstallations.id, { onDelete: `cascade` }),
    // Who completed the claim. Never used for AUTHORIZATION — but the OAuth
    // callback's self-heal (EXP-365) uses it as a cleanup SCOPE: a re-auth may
    // reap only stale zero-dependency links this same user created, never a
    // teammate's connection.
    createdByUserId: text(`created_by_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.githubInstallationId),
    index(`idx_github_installation_links_installation`).on(
      table.githubInstallationId
    ),
  ]
)

// User-scoped repo entitlements under a team ↔ installation claim
// (SERVER-ONLY, never synced). A link alone is INSTALLATION-granular, but
// GitHub attributes an installation to a user who can access even ONE of its
// repos — so a lone collaborator must not get to browse/connect the WHOLE
// installation. These rows capture what the connecting user could actually
// access, recorded at OAuth-callback time via
// `GET /user/installations/{id}/repositories` (the only moment a user-scoped
// token exists — it is transient, never persisted). A row means "team W
// may see/connect repo `fullName` under installation I because user U proved
// user-scoped GitHub access". Since EXP-557 the entitlement is ACTOR-SCOPED,
// not a union across members: effective entitlement for user U = EXISTS(grant
// for (W, I, fullName, U)) — a teammate's grant never entitles U (see
// `assertRepoGrant` in lib/trpc/integrations.ts). The per-user unique key
// makes each re-auth a clean per-user REPLACE. Keyed on GitHub's NUMERIC id
// (like repositories.installation_id) so capture never depends on link-row
// creation timing. Gates DISCOVERY (integrations.repos) and CONNECT
// (assertRepoInstallationAccess) only — never token minting.
export const githubInstallationRepoGrants = pgTable(
  `github_installation_repo_grants`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    installationId: bigint(`installation_id`, { mode: `number` }).notNull(),
    // `owner/name` as GitHub reports it.
    fullName: text(`full_name`).notNull(),
    private: boolean().notNull().default(false),
    defaultBranch: text(`default_branch`),
    // Cascade, never set-null: a grant row means "THIS user proved access".
    // A set-null here would leave an ownerless row that entitles nobody yet
    // still counts as a grant on the link — masking the stale/Disconnect
    // state forever — while being unreachable by the per-user re-auth
    // REPLACE and every other cleanup path.
    grantedByUserId: text(`granted_by_user_id`).references(() => users.id, {
      onDelete: `cascade`,
    }),
    ...timestamps,
  },
  (table) => [
    // Named explicitly: drizzle's default composite name here exceeds
    // Postgres's 63-byte identifier limit (silent truncation).
    unique(`github_installation_repo_grants_scope_unique`).on(
      table.teamId,
      table.installationId,
      table.fullName,
      table.grantedByUserId
    ),
    index(`idx_github_installation_repo_grants_ws_inst`).on(
      table.teamId,
      table.installationId
    ),
  ]
)

// GitHub account ↔ app user (SERVER-ONLY, never synced). EXP-617: the ONLY
// way to know that the human who opened or merged a pull request ON GITHUB is
// someone with an account here — a webhook's `sender`/`merged_by` carries a
// GitHub identity and nothing else, so without this the PR fan-out is
// anonymous and cannot keep the author out of their own notification.
//
// Written in exactly one place: the connect OAuth callback, where the code
// exchange proves control of the account. That means a user is only mapped
// once they have connected GitHub here; until then behaviour is the pre-617
// one. There is deliberately NO backfill from `github_installations
// .account_login` — resolving a historical login through `GET /users/{login}`
// returns whoever holds that login TODAY, i.e. the squatter after a rename,
// and would silently suppress the wrong person's notifications.
export const githubUserIdentities = pgTable(
  `github_user_identities`,
  {
    id: uuidPk(),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // GitHub's NUMERIC account id — the only rename-stable key (same reasoning
    // as repositories.installation_id). Matching is done on THIS, never on the
    // login, whenever the payload carries one.
    githubUserId: bigint(`github_user_id`, { mode: `number` }).notNull(),
    // Display/debug CACHE of the login at verification time. It goes stale the
    // moment the user renames on GitHub and is never healed; do not "helpfully"
    // add a login fallback to the resolver when an id is present.
    githubLogin: text(`github_login`).notNull(),
    verifiedAt: timestamp(`verified_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
    ...timestamps,
  },
  (table) => [
    // Unique on the GITHUB id, not on user_id: one GitHub account must resolve
    // to at most one app user (that is what makes exclusion safe), but one
    // human legitimately has work and personal accounts and both should
    // suppress their own notifications.
    unique(`uniq_github_user_identities_github_user_id`).on(table.githubUserId),
    index(`idx_github_user_identities_user`).on(table.userId),
  ]
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
    // App-written team pointer for ISSUE-LESS rows (helpdesk support_reply):
    // with no issue to resolve a team from, clients need this to route the
    // notification to the right team's Support inbox. Synced (in the shape
    // allowlist), unlike board_id. Stays NULL on issue-anchored rows — their
    // team comes from the issue.
    teamId: uuid(`team_id`).references(() => teams.id, {
      onDelete: `cascade`,
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
    // recency) — without it the issue-less team-wide fan-out walks each
    // recipient's full notification history per inbound support message.
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
  ]
)

// Team action prompts. An action is a named markdown prompt the desktop runs
// as an interactive agent session on the trunk clone (or a scratch dir when
// repo-less) — code review, backlog grooming, deploys… Synced as the 15th
// Electric shape MINUS `body` (EXP-268): the ≤64KB prompt never rides sync —
// clients list from the shape and fetch the body via tRPC `actions.get` on
// demand (editors on open, the desktop right before a run). Writes stay
// team-owner-only over tRPC; every member may list/run.
export const actions = pgTable(
  `actions`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Optional execution context: with a repo the run targets its trunk clone
    // on the default branch; without one, a scratch dir holding only the MCP
    // config. SET NULL so disconnecting a repo degrades the action to
    // repo-less instead of deleting it.
    repositoryId: uuid(`repository_id`).references(() => repositories.id, {
      onDelete: `set null`,
    }),
    name: varchar({ length: 255 }).notNull(),
    description: text(),
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
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.name),
    index(`idx_actions_team`).on(table.teamId),
  ]
)

// EXP-583: automations are their own entity — a schedule or issue-event
// trigger (`trigger` jsonb, when-part only; typed union + strict write zod in
// domain.ts) that runs ONE action on ONE device with its own agent/model/
// effort. Synced via the 19th Electric shape (team-scoped). LOCAL-ONLY by
// design: `device_id` is the steer device_id (not a row uuid) of the machine
// whose desktop/daemon watches its own sync and fires the run; there is no
// server scheduler. CASCADE on the action: an automation without its target
// is meaningless. Agent/model/effort NULL = the device's launch defaults.
export const automations = pgTable(
  `automations`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    actionId: uuid(`action_id`)
      .notNull()
      .references(() => actions.id, { onDelete: `cascade` }),
    deviceId: varchar(`device_id`, { length: 128 }).notNull(),
    enabled: boolean().notNull().default(true),
    trigger: jsonb().$type<AutomationTrigger>().notNull(),
    agent: varchar({ length: 16 }),
    model: varchar({ length: 64 }),
    effort: varchar({ length: 32 }),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    index(`idx_automations_team`).on(table.teamId),
    index(`idx_automations_action`).on(table.actionId),
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
  // Stable per-user secret embedded in one-click List-Unsubscribe links.
  unsubscribeToken: varchar(`unsubscribe_token`, { length: 64 })
    .notNull()
    .unique(),
  ...timestamps,
})

// Email audit ledger (SERVER-ONLY). One row per outbound app email — hourly
// digests, helpdesk support mail, and external widget-reporter mail (null
// userId). Per-notification email idempotency does NOT live here: it is the
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
    // notifications; the support/widget paths have none) — kept for old
    // rows' audit trail.
    notificationId: uuid(`notification_id`).references(() => notifications.id, {
      onDelete: `set null`,
    }),
    issueId: uuid(`issue_id`).references(() => issues.id, {
      onDelete: `set null`,
    }),
    // digest|support_reply|support_confirmation|widget_resolution|team_invite
    // |password_reset|email_verification|contact — documented varchar (legacy
    // rows: notification).
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
    // Where FEEDBACK-mode submissions land. NULLABLE (EXP-180): a
    // support-only widget targets no board at all — its tickets go to the
    // team support inbox. `set null` (not cascade): deleting the target
    // board degrades feedback mode, never deletes the config.
    boardId: uuid(`board_id`).references(() => boards.id, {
      onDelete: `set null`,
    }),
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
    // { buttonLabel?, accentColor?, position?, launcher?, emailRequired?,
    //   collectEmail?, collectName?, nameRequired?, customFields?, modes?,
    //   labelIds?, theme? }.
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
// Feedback submissions anchor on the created issue (`issue_id`); support
// submissions anchor on the created ticket (`support_thread_id`) — exactly
// one of the two is set.
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
      .unique()
      .references(() => issues.id, { onDelete: `cascade` }),
    supportThreadId: uuid(`support_thread_id`).references(
      () => supportThreads.id,
      { onDelete: `cascade` }
    ),
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
    ...timestamps,
  },
  (table) => [
    index(`idx_widget_submissions_config`).on(table.widgetConfigId),
    index(`idx_widget_submissions_thread`).on(table.supportThreadId),
  ]
)

// Helpdesk conversation threads (SERVER-ONLY, never Electric-synced — read
// via the `helpdesk` tRPC router and the anonymous magic-link routes). A
// ticket is a STANDALONE team-scoped record (EXP-180 — it is no longer
// backed by an issue; the whole conversation lives in these two tables, and
// a ticket only touches the issue tracker when a member explicitly escalates
// it, which files an ordinary issue and links it via linked_issue_id). The
// reporter's only credential is the token embedded in emailed magic links —
// deterministic HMAC(server secret, thread id), recomputed per email and
// verified by recompute (apps/web lib/helpdesk/token.ts), so NOTHING secret
// is stored at rest and a DB leak never leaks live conversation URLs
// (EXP-132).
export const supportThreads = pgTable(
  `support_threads`,
  {
    id: uuidPk(),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    title: varchar({ length: 500 }).notNull(),
    // 'open' | 'resolved' — documented varchar (server-only vocabulary in
    // domain.ts, not the contract), same convention as message direction/
    // visibility. Close/reopen flip this; an escalated issue's status is
    // deliberately independent.
    status: varchar({ length: 16 })
      .notNull()
      .default(`open`)
      .$type<`open` | `resolved`>(),
    // Set by the member "escalate" action: the ordinary issue created from
    // this ticket. `set null` — deleting the issue keeps the conversation.
    linkedIssueId: uuid(`linked_issue_id`).references(() => issues.id, {
      onDelete: `set null`,
    }),
    reporterEmail: varchar(`reporter_email`, { length: 320 }).notNull(),
    reporterName: varchar(`reporter_name`, { length: 255 }),
    // Stamped on close: the transcript stays readable but replies are
    // rejected. Reopen clears this — the magic link itself never changes
    // (it is recomputed from the thread id, not stored).
    tokenRevokedAt: timestamp(`token_revoked_at`, { withTimezone: true }),
    // When the reporter last loaded the magic-link page — lets members see
    // whether their reply has been read.
    lastReporterSeenAt: timestamp(`last_reporter_seen_at`, {
      withTimezone: true,
    }),
    ...timestamps,
  },
  (table) => [index(`idx_support_threads_team`).on(table.teamId)]
)

// Individual helpdesk messages. direction: inbound|outbound (inbound = the
// reporter; author_user_id NULL). visibility: public|internal — internal
// notes are member-only and never reach the reporter page or emails. Both
// documented varchars (server-only vocabulary in domain.ts, not the
// contract).
export const supportMessages = pgTable(
  `support_messages`,
  {
    id: uuidPk(),
    threadId: uuid(`thread_id`)
      .notNull()
      .references(() => supportThreads.id, { onDelete: `cascade` }),
    // NULL = the external reporter wrote it.
    authorUserId: text(`author_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    direction: varchar({ length: 16 })
      .notNull()
      .$type<`inbound` | `outbound`>(),
    visibility: varchar({ length: 16 })
      .notNull()
      .default(`public`)
      .$type<`public` | `internal`>(),
    // Plain text on both sides: reporter input is untrusted (never rendered
    // as GFM, no @mention/#ref resolution), and member replies land in plain
    // emails, so symmetrical plain text keeps the transcript honest.
    body: text().notNull(),
    // The outbound email that carried this reply (audit; NULL for internal
    // notes, inbound messages, and no-transport sends).
    emailDeliveryId: uuid(`email_delivery_id`).references(
      () => emailDeliveries.id,
      { onDelete: `set null` }
    ),
    ...timestamps,
  },
  (table) => [index(`idx_support_messages_thread`).on(table.threadId)]
)

// What an OAuth-authenticated MCP client may touch (SERVER-ONLY, written by
// the /auth/consent page). One row per (user, oauth client); re-consenting
// replaces the selection. `teamIds` grants whole teams (including
// boards created later); `boardIds` grants individual boards. A token
// whose (user, client) pair has NO row gets no access — the holder must
// re-authenticate through the consent page. Session-cookie and personal
// api-key access to /api/mcp is never grant-scoped (the user's own
// credentials keep full membership access).
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
  prState: prStateSchema.nullable(),
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

export const selectIssueSubscriberSchema = createSelectSchema(
  issueSubscribers,
  {
    source: subscriberSourceSchema,
  }
)

export const selectIssueEventSchema = createSelectSchema(issueEvents, {
  type: issueEventTypeSchema,
})

export const selectCodingSessionSchema = createSelectSchema(codingSessions, {
  status: codingSessionStatusSchema,
})

export const selectRepositorySchema = createSelectSchema(repositories)

export const selectActionSchema = createSelectSchema(actions, {
  inputs: actionInputsSchema,
})

export const selectAutomationSchema = createSelectSchema(automations, {
  // TOLERANT read (unlike the strict write union in domain.ts): the web
  // automations collection must not brick on a future trigger kind, so runtime
  // validation only checks "object" — clients parse triggers leniently
  // (parseAutomationTrigger) and treat unknown shapes as "never fires".
  trigger: z.custom<AutomationTrigger>(
    (value) =>
      value !== null && typeof value === `object` && !Array.isArray(value)
  ),
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
  launchDefaults: deviceLaunchDefaultsSchema.nullable(),
  agentAccounts: deviceAgentAccountsSchema.nullable(),
  agentUsage: deviceAgentUsageSchema.nullable(),
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
    sharedTeamId: true,
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
export type SessionAttachment = InferSelectModel<typeof sessionAttachments>

export type User = InferSelectModel<typeof users>
export type Notification = InferSelectModel<typeof notifications>
export type IssueSubscriber = InferSelectModel<typeof issueSubscribers>
export type IssueEvent = InferSelectModel<typeof issueEvents>
export type CodingSession = InferSelectModel<typeof codingSessions>
export type Repository = InferSelectModel<typeof repositories>
export type Action = InferSelectModel<typeof actions>
export type Automation = InferSelectModel<typeof automations>
export type SyncedAction = Omit<Action, `body`>
export type UserNotificationPrefs = InferSelectModel<
  typeof userNotificationPrefs
>
export type EmailDelivery = InferSelectModel<typeof emailDeliveries>
export type ConversionEvent = InferSelectModel<typeof conversionEvents>
export type WidgetConfig = InferSelectModel<typeof widgetConfigs>
export type WidgetSubmission = InferSelectModel<typeof widgetSubmissions>
export type SupportThread = InferSelectModel<typeof supportThreads>
export type SupportMessage = InferSelectModel<typeof supportMessages>
export type McpGrant = InferSelectModel<typeof mcpGrants>
export type Device = InferSelectModel<typeof devices>
export type DeviceWorktree = InferSelectModel<typeof deviceWorktrees>
export type SyncedDeviceWorktree = Omit<
  DeviceWorktree,
  `userId` | `sharedTeamId`
>
export type DeviceCommand = InferSelectModel<typeof deviceCommands>

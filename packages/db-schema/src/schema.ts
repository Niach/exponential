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
  time,
  timestamp,
  unique,
  uniqueIndex,
  uuid,
  varchar,
  boolean,
  date,
} from "drizzle-orm/pg-core"
import { sql, type InferSelectModel } from "drizzle-orm"
import { createSchemaFactory } from "drizzle-zod"
import { z } from "zod"
import {
  codingSessionStatusSchema,
  codingSessionStatusValues,
  commentBodySchema,
  issueDescriptionSchema,
  issueEventTypeSchema,
  issueEventTypeValues,
  issuePrioritySchema,
  issuePriorityValues,
  issueSourceSchema,
  issueSourceValues,
  issueStatusSchema,
  issueStatusValues,
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

export const issueStatusEnum = pgEnum(`issue_status`, issueStatusValues)

export const issuePriorityEnum = pgEnum(`issue_priority`, issuePriorityValues)

export const issueSourceEnum = pgEnum(`issue_source`, issueSourceValues)

export const notificationTypeEnum = pgEnum(
  `notification_type`,
  notificationTypeValues
)

export const teamMemberRoleEnum = pgEnum(
  `team_member_role`,
  teamRoleValues
)


export const prStateEnum = pgEnum(`pr_state`, prStateValues)

export const codingSessionStatusEnum = pgEnum(
  `coding_session_status`,
  codingSessionStatusValues
)

export const issueEventTypeEnum = pgEnum(
  `issue_event_type`,
  issueEventTypeValues
)

export const subscriberSourceEnum = pgEnum(
  `subscriber_source`,
  subscriberSourceValues
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
  // Team-level helpdesk switch (EXP-180 — replaced the per-board flag;
  // Pro-gated on cloud via assertCanUseHelpdesk on enable and per submission).
  // Synced so every client can gate its Support-inbox menu entry; the
  // conversation tables themselves stay server-only.
  helpdeskEnabled: boolean(`helpdesk_enabled`).notNull().default(false),
  ...timestamps,
})

// Better Auth's Drizzle adapter resolves models by snake_case key, so this
// must be exported as `creem_subscriptions` (not camelCase). It lives here
// (rather than in auth-schema.ts) so its `team_id` FK can reference
// `teams` locally — auth-schema.ts must NOT import schema.ts (that edge
// forms an eval-time circular import that crashes `createSelectSchema`).
export const creem_subscriptions = pgTable(`creem_subscriptions`, {
  id: text(`id`).primaryKey(),
  productId: text(`product_id`).notNull(),
  referenceId: text(`reference_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  creemCustomerId: text(`creem_customer_id`),
  creemSubscriptionId: text(`creem_subscription_id`),
  creemOrderId: text(`creem_order_id`),
  // v5 per-seat binding: a subscription belongs to exactly one team, and
  // `seats` is the purchased quantity (Creem checkout `units`). Both are bound
  // from checkout metadata on the webhook path (lib/billing/creem-binding.ts);
  // the plugin's own persistence never writes these columns, so later webhook
  // updates cannot clobber them. `set null` keeps the billing history row if
  // the team is deleted.
  teamId: uuid(`team_id`).references(() => teams.id, {
    onDelete: `set null`,
  }),
  seats: integer(`seats`).default(1).notNull(),
  status: text(`status`).$defaultFn(() => `pending`).notNull(),
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
})

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
    prefix: varchar({ length: 10 }).notNull(),
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
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
    // Soft-delete (trash) marker. Non-null = trashed; the purge sweep hard-deletes
    // it (cascade) once deletedAt + BOARD_TRASH_RETENTION_MS has passed. Purge
    // time is computed, never stored (constant retention). Trashed boards drop
    // out of every membership/public scope but keep their rows for restore.
    deletedAt: timestamp(`deleted_at`, { withTimezone: true }),
    // Non-deletable marker (the dogfood board). Set by bootstrap; guards
    // in boards.delete/update and the purge sweep refuse to touch it. A synced
    // column (not a server-only id comparison) so clients can grey out the
    // affordance and it survives restore-from-backup id changes.
    isProtected: boolean(`is_protected`).notNull().default(false),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.slug),
    index(`idx_boards_repository`).on(table.repositoryId),
    // Serves the purge sweep + trash-aware shape filter; near-empty in steady
    // state (only trashed rows are indexed).
    index(`idx_boards_deleted`)
      .on(table.deletedAt)
      .where(sql`deleted_at IS NOT NULL`),
  ]
)

export const issues = pgTable(
  `issues`,
  {
    id: uuidPk(),
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    number: integer().notNull().default(0),
    identifier: varchar({ length: 20 }).notNull().default(``),
    title: varchar({ length: 500 }).notNull(),
    // Plain GFM markdown (was jsonb `{ text }`).
    description: text(),
    status: issueStatusEnum().notNull().default(`backlog`),
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
    dueTime: time(`due_time`),
    endTime: time(`end_time`),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    completedAt: timestamp(`completed_at`, { withTimezone: true }),
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
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
    index(`idx_issues_assignee`).on(table.assigneeId),
    index(`idx_issues_due_date`).on(table.dueDate),
    // Backstop under generate_issue_number()'s counter allocator (see
    // issue_number_counters below): any residual allocation race fails loudly
    // instead of committing two issues with the same identifier.
    uniqueIndex(`uniq_issues_board_number`).on(table.boardId, table.number),
  ]
)

// Per-board monotonic issue-number allocator — server-only, NEVER
// Electric-synced (no shape proxy; proxy count stays 14). The
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
  (table) => [index(`idx_labels_team`).on(table.teamId)]
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
    // Denormalized from issue→board by populate_issue_child_board_id so
    // anonymous feedback-board shape filters stay board-scoped (Electric
    // where clauses are single-table).
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
  },
  (table) => [
    primaryKey({ columns: [table.issueId, table.labelId] }),
    index(`idx_issue_labels_label`).on(table.labelId),
    index(`idx_issue_labels_team`).on(table.teamId),
    index(`idx_issue_labels_board`).on(table.boardId),
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
    // Denormalized from issue→board (populate_issue_child_board_id) for
    // board-scoped anonymous feedback-board shape filters.
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    authorId: text(`author_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // Plain GFM markdown (was jsonb `{ text }`).
    body: text().notNull(),
    editedAt: timestamp(`edited_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_comments_issue`).on(table.issueId),
    index(`idx_comments_team`).on(table.teamId),
    index(`idx_comments_board`).on(table.boardId),
  ]
)

// The live "coding now" record — one row per interactive desktop coding
// session (one terminal tab + one `claude` child). SYNCED as an Electric shape
// so every coordination client shows the badge + Watch/Steer button. No
// plan/approval state, no run history, no slot pool — PR outcome lives on
// `issues` (prUrl/prNumber/prState/branch). Two session subjects: issue-scoped
// (`issue_id` set — one worktree, one issue) and batch-scoped (`issue_id` and
// `board_id` NULL, `team_id` written directly — the desktop multi-issue
// batch run). Enforced by the tRPC writer (exactly one of issueId/teamId
// in the start input). `team_id` is denormalized from issue→board by
// trigger for issue rows (the populate triggers no-op when issue_id IS NULL).
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
    // Denormalized from issue→board (populate_issue_child_board_id) for
    // board-scoped anonymous feedback-board shape filters. Nullable: a
    // batch-scoped session spans boards (never anonymous-visible — the
    // anonymous board_id clause can't match NULL).
    boardId: uuid(`board_id`).references(() => boards.id, {
      onDelete: `cascade`,
    }),
    // The real user driving the session under their own auth — NOT a synthetic
    // agent identity.
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // Human label of the host device ("Dennis's MacBook"), shown on the badge.
    deviceLabel: varchar(`device_label`, { length: 255 }),
    status: codingSessionStatusEnum().notNull().default(`running`),
    // Desktop-written attention flag (EXP-214): the agent is parked on a
    // plan-approval or AskUserQuestion picker and waits for a human. Composes
    // with running/in_review (which stay server-owned) instead of being a
    // status of its own; cleared by the desktop when the picker resolves.
    needsInput: boolean(`needs_input`).notNull().default(false),
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
    // Denormalized from issue→board (populate_issue_child_board_id) for
    // board-scoped anonymous feedback-board shape filters + the public
    // attachment-bytes read path.
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    commentId: uuid(`comment_id`).references(() => comments.id, {
      onDelete: `set null`,
    }),
    // NULLABLE: widget screenshot attachments have no user uploader (the
    // synthetic per-widget bot user was removed). Still `cascade` for real
    // uploaders — deleting a user reclaims the attachments they uploaded.
    uploaderId: text(`uploader_id`).references(() => users.id, {
      onDelete: `cascade`,
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
  (table) => [unique().on(table.token, table.userId)]
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
  ...timestamps,
})

// Team ↔ GitHub App installation claims (SERVER-ONLY, never synced).
// A link means "this team may browse/connect this installation's repos".
// Created by the OAuth claim flow (or the install-page round-trip fallback) —
// both prove control of the GitHub account before linking. Many-to-many: one
// org install can serve several teams, one team can link several
// GitHub accounts. CASCADE on the installation FK: when an uninstall webhook
// deletes the github_installations row, its links vanish with it.
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
    // Audit only — who completed the claim; never used for authorization.
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
// user-scoped GitHub access". Effective entitlement = EXISTS(any grant for
// (W, I, fullName)) — union across members; the per-user unique key makes each
// re-auth a clean per-user REPLACE. Keyed on GitHub's NUMERIC installation id
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
    // Cascade, never set-null: a grant row means "THIS user proved access",
    // and the entitlement check (assertRepoGrant) matches on
    // team+installation+repo alone. A set-null here would leave an
    // ownerless row that keeps entitling the team to the departed
    // user's private repos while being unreachable by the per-user re-auth
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
    // notifications shape can hide rows of trashed boards for the 48h
    // trash window. Server-only scoping — excluded from the shape via its
    // columns allowlist, like emailed_at. Nullable like issue_id: an
    // issue-less notification carries no board identity.
    boardId: uuid(`board_id`).references(() => boards.id, {
      onDelete: `cascade`,
    }),
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
    // The hourly digest sweep's scan: unread, never-emailed rows by age.
    index(`idx_notifications_digest_pending`)
      .on(table.createdAt)
      .where(sql`read_at IS NULL AND emailed_at IS NULL`),
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
    // Retained for notification fan-out and team-level queries; the Electric
    // shape filter is board-scoped (see the board_id column below) so a
    // trashed board's subscriptions drop out of member sync.
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    // Denormalized from issue→board (populate_issue_child_board_id).
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
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
    private: boolean().notNull().default(false),
    // Cached GitHub App installation id; nullable — the App JWT can still
    // resolve it on demand (github-app.ts is storage-free).
    installationId: bigint(`installation_id`, { mode: `number` }),
    // The App lost access to this repo (installation_repositories webhook, or a
    // verified token mint failed). NULL = accessible as far as we know. Cleared
    // by connect, a webhook re-grant, and the list heal pass.
    inaccessibleAt: timestamp(`inaccessible_at`, { withTimezone: true }),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.teamId, table.fullName),
    index(`idx_repositories_team`).on(table.teamId),
  ]
)

// Per-board terminal run commands (SERVER-ONLY, tRPC-managed — never an
// Electric shape; the proxy count stays 14). A run config is just a named
// argv the desktop apps spawn into a terminal tab (run configs live in the
// DATABASE, not the repo). SECURITY: because this is DB-stored argv
// executed locally, desktops MUST keep the per-device Trust & Run
// commandSetHash prompt and re-hash whenever the fetched config set changes —
// never auto-run synced values. `team_id` is denormalized from the
// board server-side on insert (tRPC-only writes, so no trigger needed).
export const runConfigs = pgTable(
  `run_configs`,
  {
    id: uuidPk(),
    boardId: uuid(`board_id`)
      .notNull()
      .references(() => boards.id, { onDelete: `cascade` }),
    teamId: uuid(`team_id`)
      .notNull()
      .references(() => teams.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    // Program + arguments, spawned as-is (no shell). At least one element.
    argv: jsonb().$type<string[]>().notNull(),
    // Working directory relative to the repo root; null = repo root. The
    // server rejects absolute paths and `..` segments.
    cwd: text(),
    // Extra environment. PATH/LD_PRELOAD/DYLD_* are stripped server-side.
    env: jsonb()
      .$type<Record<string, string>>()
      .notNull()
      .default(sql`'{}'::jsonb`),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    unique().on(table.boardId, table.name),
    index(`idx_run_configs_team`).on(table.teamId),
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
  // Stable per-user secret embedded in one-click List-Unsubscribe links.
  unsubscribeToken: varchar(`unsubscribe_token`, { length: 64 })
    .notNull()
    .unique(),
  ...timestamps,
})

// Email audit + idempotency ledger (SERVER-ONLY). One delivery per
// notification row; also home for external widget-reporter mail (null userId).
export const emailDeliveries = pgTable(
  `email_deliveries`,
  {
    id: uuidPk(),
    // Nullable: external widget reporters have no users row.
    userId: text(`user_id`).references(() => users.id, { onDelete: `cascade` }),
    toEmail: varchar(`to_email`, { length: 320 }).notNull(),
    // Idempotency key: one delivery per notification row.
    notificationId: uuid(`notification_id`).references(() => notifications.id, {
      onDelete: `set null`,
    }),
    issueId: uuid(`issue_id`).references(() => issues.id, {
      onDelete: `set null`,
    }),
    // notification|digest|widget_resolution — documented varchar.
    kind: varchar({ length: 32 }).notNull(),
    // queued|sent|failed|bounced|complained — documented varchar (the last
    // two are stamped post-send by the SES feedback webhook).
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
  // bounce|complaint — the LAST event's kind (documented varchar).
  kind: varchar({ length: 16 }).notNull(),
  // SES bounce classification of the last event (e.g. Permanent/General);
  // complaints carry the feedback type in bounceSubType.
  bounceType: varchar(`bounce_type`, { length: 32 }),
  bounceSubType: varchar(`bounce_sub_type`, { length: 64 }),
  diagnostic: text(),
  eventCount: integer(`event_count`).notNull().default(1),
  lastEventAt: timestamp(`last_event_at`, { withTimezone: true }).notNull(),
  suppressedAt: timestamp(`suppressed_at`, { withTimezone: true }),
  ...timestamps,
})

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
    // Appearance/behavior overrides served to the widget loader:
    // { buttonLabel?, accentColor?, position?, emailRequired? }.
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

export const selectTeamMemberSchema = createSelectSchema(
  teamMembers,
  {
    role: teamRoleSchema,
  }
)
export const selectTeamInviteSchema = createSelectSchema(
  teamInvites,
  {
    role: teamRoleSchema,
  }
)
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
  createdAt: true,
  updatedAt: true,
})

export const selectLabelSchema = createSelectSchema(labels)
export const createLabelSchema = createInsertSchema(labels).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectIssueLabelSchema = createSelectSchema(issueLabels)

export const selectUserSchema = createSelectSchema(users)

export const selectCommentSchema = createSelectSchema(comments, {
  body: commentBodySchema,
})
export const createCommentSchema = createInsertSchema(comments).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectAttachmentSchema = createSelectSchema(attachments)

export const selectNotificationSchema = createSelectSchema(notifications)

export const selectIssueSubscriberSchema = createSelectSchema(issueSubscribers, {
  source: subscriberSourceSchema,
})

export const selectIssueEventSchema = createSelectSchema(issueEvents, {
  type: issueEventTypeSchema,
})

export const selectCodingSessionSchema = createSelectSchema(codingSessions, {
  status: codingSessionStatusSchema,
})

export const selectRepositorySchema = createSelectSchema(repositories)

export const selectRunConfigSchema = createSelectSchema(runConfigs, {
  argv: z.array(z.string()),
  env: z.record(z.string(), z.string()),
})

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

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type Team = InferSelectModel<typeof teams>
export type TeamMember = InferSelectModel<typeof teamMembers>
export type TeamInvite = InferSelectModel<typeof teamInvites>
export type Board = InferSelectModel<typeof boards>
export type Issue = InferSelectModel<typeof issues>
export type Label = InferSelectModel<typeof labels>
export type IssueLabel = InferSelectModel<typeof issueLabels>
export type Comment = InferSelectModel<typeof comments>
export type Attachment = InferSelectModel<typeof attachments>

export type User = InferSelectModel<typeof users>
export type Notification = InferSelectModel<typeof notifications>
export type IssueSubscriber = InferSelectModel<typeof issueSubscribers>
export type IssueEvent = InferSelectModel<typeof issueEvents>
export type CodingSession = InferSelectModel<typeof codingSessions>
export type Repository = InferSelectModel<typeof repositories>
export type RunConfig = InferSelectModel<typeof runConfigs>
export type UserNotificationPrefs = InferSelectModel<
  typeof userNotificationPrefs
>
export type EmailDelivery = InferSelectModel<typeof emailDeliveries>
export type WidgetConfig = InferSelectModel<typeof widgetConfigs>
export type WidgetSubmission = InferSelectModel<typeof widgetSubmissions>
export type SupportThread = InferSelectModel<typeof supportThreads>
export type SupportMessage = InferSelectModel<typeof supportMessages>
export type McpGrant = InferSelectModel<typeof mcpGrants>

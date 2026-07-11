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
  issueStatusSchema,
  issueStatusValues,
  type NotificationType,
  notificationTypeValues,
  prStateSchema,
  prStateValues,
  projectTypeSchema,
  projectTypeValues,
  publicCodingVisibilitySchema,
  publicCodingVisibilityValues,
  recurrenceUnitSchema,
  recurrenceUnitValues,
  subscriberSourceSchema,
  subscriberSourceValues,
  workspaceRoleSchema,
  workspaceRoleValues,
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

export const notificationTypeEnum = pgEnum(
  `notification_type`,
  notificationTypeValues
)

export const workspaceMemberRoleEnum = pgEnum(
  `workspace_member_role`,
  workspaceRoleValues
)

export const projectTypeEnum = pgEnum(`project_type`, projectTypeValues)

export const publicCodingVisibilityEnum = pgEnum(
  `public_coding_visibility`,
  publicCodingVisibilityValues
)

export const recurrenceUnitEnum = pgEnum(
  `recurrence_unit`,
  recurrenceUnitValues
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

// Workspaces are ALWAYS private (v7): publicness moved to the project level
// (projects.type = 'feedback'). The old is_public/public_write_policy columns
// and the self-service join flow are gone — membership is invite-only.
export const workspaces = pgTable(`workspaces`, {
  id: uuidPk(),
  name: varchar({ length: 255 }).notNull(),
  slug: varchar({ length: 255 }).notNull().unique(),
  iconUrl: text(`icon_url`),
  // Admin-granted complimentary tier ('pro' | 'business' | 'unlimited').
  // SERVER-ONLY — must stay behind the workspaces shape columns allowlist.
  // Honored by getWorkspacePlan as a floor over the Creem-derived tier.
  compTier: text(`comp_tier`),
  ...timestamps,
})

// Better Auth's Drizzle adapter resolves models by snake_case key, so this
// must be exported as `creem_subscriptions` (not camelCase). It lives here
// (rather than in auth-schema.ts) so its `workspace_id` FK can reference
// `workspaces` locally — auth-schema.ts must NOT import schema.ts (that edge
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
  // v5 per-seat binding: a subscription belongs to exactly one workspace, and
  // `seats` is the purchased quantity (Creem checkout `units`). Both are bound
  // from checkout metadata on the webhook path (lib/billing/creem-binding.ts);
  // the plugin's own persistence never writes these columns, so later webhook
  // updates cannot clobber them. `set null` keeps the billing history row if
  // the workspace is deleted.
  workspaceId: uuid(`workspace_id`).references(() => workspaces.id, {
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

export const workspaceMembers = pgTable(
  `workspace_members`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    role: workspaceMemberRoleEnum().notNull().default(`member`),
    ...timestamps,
  },
  (table) => [
    unique().on(table.workspaceId, table.userId),
    index(`idx_workspace_members_user`).on(table.userId),
  ]
)

export const workspaceInvites = pgTable(`workspace_invites`, {
  id: uuidPk(),
  workspaceId: uuid(`workspace_id`)
    .notNull()
    .references(() => workspaces.id, { onDelete: `cascade` }),
  invitedById: text(`invited_by_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  role: workspaceMemberRoleEnum().notNull().default(`member`),
  token: varchar({ length: 255 }).notNull().unique(),
  acceptedAt: timestamp(`accepted_at`, { withTimezone: true }),
  expiresAt: timestamp(`expires_at`, { withTimezone: true }).notNull(),
  ...timestamps,
})

export const projects = pgTable(
  `projects`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    slug: varchar({ length: 255 }).notNull(),
    prefix: varchar({ length: 10 }).notNull(),
    color: varchar({ length: 7 }).notNull().default(`#6366f1`),
    // What this project IS (v7): `dev` = repo-backed coding project (repository
    // required), `tasks` = plain issue tracking (no repo), `feedback` = PUBLIC
    // read-only board (anonymous browsing; writes only via the embedded
    // widget). Coding features gate on repo presence, not type.
    type: projectTypeEnum().notNull().default(`dev`),
    // Anonymous-visitor visibility toggles. Only meaningful when
    // type='feedback' — every public-scope query gates on the type first, so
    // stale values on private projects are inert.
    publicShowComments: boolean(`public_show_comments`).notNull().default(true),
    publicShowActivity: boolean(`public_show_activity`)
      .notNull()
      .default(false),
    publicShowCoding: publicCodingVisibilityEnum(`public_show_coding`)
      .notNull()
      .default(`off`),
    // A `dev` project is backed by exactly one repo from the workspace
    // registry; the desktop launcher clones this. Nullable since v7: `tasks`
    // and `feedback` projects need no repo (a feedback board MAY still have
    // one — the dogfood board is feedback + repo-backed). `restrict` (not
    // cascade): a repo that still backs a project can't be deleted — retarget
    // or delete the projects first. One repo may back several projects
    // (monorepo); plan limits still count registry rows.
    repositoryId: uuid(`repository_id`).references(() => repositories.id, {
      onDelete: `restrict`,
    }),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
    // Soft-delete (trash) marker. Non-null = trashed; the purge sweep hard-deletes
    // it (cascade) once deletedAt + PROJECT_TRASH_RETENTION_MS has passed. Purge
    // time is computed, never stored (constant retention). Trashed projects drop
    // out of every membership/public scope but keep their rows for restore.
    deletedAt: timestamp(`deleted_at`, { withTimezone: true }),
    // Non-deletable marker (the dogfood feedback board). Set by bootstrap; guards
    // in projects.delete/update and the purge sweep refuse to touch it. A synced
    // column (not a server-only id comparison) so clients can grey out the
    // affordance and it survives restore-from-backup id changes.
    isProtected: boolean(`is_protected`).notNull().default(false),
    ...timestamps,
  },
  (table) => [
    unique().on(table.workspaceId, table.slug),
    index(`idx_projects_repository`).on(table.repositoryId),
    // Serves the anonymous public-scope resolver (getPublicProjectScope).
    index(`idx_projects_feedback`)
      .on(table.type)
      .where(sql`type = 'feedback'`),
    // Serves the purge sweep + trash-aware shape filter; near-empty in steady
    // state (only trashed rows are indexed).
    index(`idx_projects_deleted`)
      .on(table.deletedAt)
      .where(sql`deleted_at IS NOT NULL`),
  ]
)

export const issues = pgTable(
  `issues`,
  {
    id: uuidPk(),
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
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
    creatorId: text(`creator_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    dueDate: date(`due_date`),
    dueTime: time(`due_time`),
    endTime: time(`end_time`),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    completedAt: timestamp(`completed_at`, { withTimezone: true }),
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
    recurrenceInterval: integer(`recurrence_interval`),
    recurrenceUnit: recurrenceUnitEnum(`recurrence_unit`),
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
    // Release membership (EXP-56): an issue ships in at most ONE release
    // (1:N — no join table, no extra shape). Member-only: the anonymous
    // issues columns allowlist deliberately omits it.
    releaseId: uuid(`release_id`).references(() => releases.id, {
      onDelete: `set null`,
    }),
    ...timestamps,
  },
  (table) => [
    index(`idx_issues_project_status`).on(table.projectId, table.status),
    index(`idx_issues_assignee`).on(table.assigneeId),
    index(`idx_issues_due_date`).on(table.dueDate),
    index(`idx_issues_release`).on(table.releaseId),
    // Backstop under generate_issue_number()'s counter allocator (see
    // issue_number_counters below): any residual allocation race fails loudly
    // instead of committing two issues with the same identifier.
    uniqueIndex(`uniq_issues_project_number`).on(table.projectId, table.number),
  ]
)

// Per-project monotonic issue-number allocator — server-only, NEVER
// Electric-synced (no shape proxy; proxy count stays 14). The
// generate_issue_number() trigger (custom trigger file, re-applied at every
// boot by bootstrap-cloud applyCustomSql) increments this row under its row
// lock: serializes concurrent inserts (no duplicate numbers) and never
// decreases (deleting the top-numbered issue can't recycle its identifier —
// #PREFIX-n mentions and exp/PREFIX-n branches stay unambiguous). Keyed 1:1 by
// project on purpose (deliberate deviation from the uuid-surrogate-PK
// convention). No zod/insert-schema exports — no TS code queries it; only the
// trigger touches it.
export const issueNumberCounters = pgTable(`issue_number_counters`, {
  projectId: uuid(`project_id`)
    .primaryKey()
    .references(() => projects.id, { onDelete: `cascade` }),
  counter: integer().notNull(),
  ...timestamps,
})

export const labels = pgTable(`labels`, {
  id: uuidPk(),
  workspaceId: uuid(`workspace_id`)
    .notNull()
    .references(() => workspaces.id, { onDelete: `cascade` }),
  name: varchar({ length: 255 }).notNull(),
  color: varchar({ length: 7 }).notNull().default(`#6366f1`),
  sortOrder: doublePrecision(`sort_order`).notNull().default(0),
  ...timestamps,
})

export const issueLabels = pgTable(
  `issue_labels`,
  {
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    labelId: uuid(`label_id`)
      .notNull()
      .references(() => labels.id, { onDelete: `cascade` }),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    // Denormalized from issue→project by populate_issue_child_project_id so
    // anonymous feedback-board shape filters stay project-scoped (Electric
    // where clauses are single-table).
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
  },
  (table) => [
    primaryKey({ columns: [table.issueId, table.labelId] }),
    index(`idx_issue_labels_label`).on(table.labelId),
    index(`idx_issue_labels_workspace`).on(table.workspaceId),
    index(`idx_issue_labels_project`).on(table.projectId),
  ]
)

export const comments = pgTable(
  `comments`,
  {
    id: uuidPk(),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    // Denormalized from issue→project (populate_issue_child_project_id) for
    // project-scoped anonymous feedback-board shape filters.
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
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
    index(`idx_comments_workspace`).on(table.workspaceId),
    index(`idx_comments_project`).on(table.projectId),
  ]
)

// The live "coding now" record — one row per interactive desktop coding
// session (one terminal tab + one `claude` child). SYNCED as an Electric shape
// so every coordination client shows the badge + Watch/Steer button. No
// plan/approval state, no run history, no slot pool — PR outcome lives on
// `issues` (prUrl/prNumber/prState/branch). Two session subjects (EXP-56):
// issue-scoped (`issue_id` set — one worktree, one issue) and release-scoped
// (`release_id` set, `issue_id`/`project_id` NULL — the desktop release
// orchestrator run). Exactly one of issue_id/release_id is set, enforced by
// the tRPC writer. `workspace_id` is denormalized from issue→project by
// trigger for issue rows and written directly from the release for release
// rows (the populate triggers no-op when issue_id IS NULL).
export const codingSessions = pgTable(
  `coding_sessions`,
  {
    id: uuidPk(),
    // Nullable since EXP-56: NULL for release-scoped orchestrator sessions.
    issueId: uuid(`issue_id`).references(() => issues.id, {
      onDelete: `cascade`,
    }),
    // Set for release-scoped sessions; NULL for issue-scoped ones.
    releaseId: uuid(`release_id`).references(() => releases.id, {
      onDelete: `cascade`,
    }),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    // Denormalized from issue→project (populate_issue_child_project_id) for
    // project-scoped anonymous feedback-board shape filters. Nullable since
    // EXP-56: a release-scoped session spans projects (never anonymous-visible
    // — the anonymous project_id clause can't match NULL).
    projectId: uuid(`project_id`).references(() => projects.id, {
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
    startedAt: timestamp(`started_at`, { withTimezone: true })
      .notNull()
      .defaultNow(),
    endedAt: timestamp(`ended_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_coding_sessions_issue`).on(table.issueId),
    index(`idx_coding_sessions_release`).on(table.releaseId),
    index(`idx_coding_sessions_workspace`).on(table.workspaceId),
    index(`idx_coding_sessions_project`).on(table.projectId),
    index(`idx_coding_sessions_user`).on(table.userId),
  ]
)

// Releases (EXP-56): workspace-level bundles of issues — the 15th Electric
// shape, synced to all four clients (MEMBER-ONLY: the shape proxy's anonymous
// clause is the impossible-match sentinel; releases never appear on public
// feedback boards). No status enum: state derives from `shipped_at` plus the
// member issues' progress (done / (total − cancelled − duplicate)). The
// desktop release-coding orchestrator builds a pushed integration branch
// `exp/rel-<slug>` per run, per-issue PRs target it, and ONE release PR
// (integration → default) is recorded here via the `exponential_release_pr_open`
// MCP tool; the GitHub webhook resolves release PRs by exact `pr_url` match
// and AUTO-SHIPS (stamps shipped_at) when that PR merges. A multi-repo release
// records only its first release PR (documented v1 limitation).
export const releases = pgTable(
  `releases`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    // Plain GFM markdown, like issue descriptions.
    description: text(),
    targetDate: date(`target_date`),
    // Non-null = shipped. Set manually (markShipped) or by the webhook when
    // the release PR merges. Independent of progress — shipping early is fine.
    shippedAt: timestamp(`shipped_at`, { withTimezone: true }),
    // Audit only (mirrors issue_events.actor_user_id) — never authorization.
    createdBy: text(`created_by`).references(() => users.id, {
      onDelete: `set null`,
    }),
    // The release PR (integration branch → default). Same shape as the issue
    // PR fields; written by exponential_release_pr_open + the webhook/poller.
    prUrl: text(`pr_url`),
    prNumber: integer(`pr_number`),
    prState: prStateEnum(`pr_state`),
    prMergedAt: timestamp(`pr_merged_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [index(`idx_releases_workspace`).on(table.workspaceId)]
)

export const attachments = pgTable(
  `attachments`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    // Denormalized from issue→project (populate_issue_child_project_id) for
    // project-scoped anonymous feedback-board shape filters + the public
    // attachment-bytes read path.
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
    commentId: uuid(`comment_id`).references(() => comments.id, {
      onDelete: `set null`,
    }),
    uploaderId: text(`uploader_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
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
    index(`idx_attachments_workspace`).on(table.workspaceId),
    index(`idx_attachments_project`).on(table.projectId),
  ]
)

export const fcmTokens = pgTable(`fcm_tokens`, {
  id: uuidPk(),
  userId: text(`user_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  token: text().notNull().unique(),
  platform: varchar({ length: 20 }).notNull(),
  ...timestamps,
})

// GitHub App installations (server-only, not synced). Mirrored from the setup
// redirect, the OAuth claim callback, and installation webhooks; token
// resolution itself is storage-free (the App JWT looks up a repo's installation
// on demand). Visibility is granted per workspace via githubInstallationLinks —
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

// Workspace ↔ GitHub App installation claims (SERVER-ONLY, never synced).
// A link means "this workspace may browse/connect this installation's repos".
// Created by the OAuth claim flow (or the install-page round-trip fallback) —
// both prove control of the GitHub account before linking. Many-to-many: one
// org install can serve several workspaces, one workspace can link several
// GitHub accounts. CASCADE on the installation FK: when an uninstall webhook
// deletes the github_installations row, its links vanish with it.
export const githubInstallationLinks = pgTable(
  `github_installation_links`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
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
    unique().on(table.workspaceId, table.githubInstallationId),
    index(`idx_github_installation_links_installation`).on(
      table.githubInstallationId
    ),
  ]
)

// User-scoped repo entitlements under a workspace ↔ installation claim
// (SERVER-ONLY, never synced). A link alone is INSTALLATION-granular, but
// GitHub attributes an installation to a user who can access even ONE of its
// repos — so a lone collaborator must not get to browse/connect the WHOLE
// installation. These rows capture what the connecting user could actually
// access, recorded at OAuth-callback time via
// `GET /user/installations/{id}/repositories` (the only moment a user-scoped
// token exists — it is transient, never persisted). A row means "workspace W
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
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    installationId: bigint(`installation_id`, { mode: `number` }).notNull(),
    // `owner/name` as GitHub reports it.
    fullName: text(`full_name`).notNull(),
    private: boolean().notNull().default(false),
    defaultBranch: text(`default_branch`),
    grantedByUserId: text(`granted_by_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    ...timestamps,
  },
  (table) => [
    // Named explicitly: drizzle's default composite name here exceeds
    // Postgres's 63-byte identifier limit (silent truncation).
    unique(`github_installation_repo_grants_scope_unique`).on(
      table.workspaceId,
      table.installationId,
      table.fullName,
      table.grantedByUserId
    ),
    index(`idx_github_installation_repo_grants_ws_inst`).on(
      table.workspaceId,
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
    // Denormalized from issue→project by populate_issue_subscriber_workspace_id
    // so the Electric shape filter stays workspace-scoped (stable, no 409 churn).
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    // Denormalized from issue→project (populate_issue_child_project_id).
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
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
    index(`idx_issue_subscribers_workspace`).on(table.workspaceId),
    index(`idx_issue_subscribers_project`).on(table.projectId),
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
    // Denormalized from issue→project by populate_issue_event_workspace_id.
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    // Denormalized from issue→project (populate_issue_child_project_id).
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
    actorUserId: text(`actor_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    type: issueEventTypeEnum().notNull(),
    payload: jsonb(),
    ...timestamps,
  },
  (table) => [
    index(`idx_issue_events_issue`).on(table.issueId),
    index(`idx_issue_events_workspace`).on(table.workspaceId),
    index(`idx_issue_events_project`).on(table.projectId),
  ]
)

// Workspace repository registry (SERVER-ONLY, tRPC-managed — never an Electric
// shape). One row per connected GitHub repo; the desktop "Start coding"
// launcher resolves its clone target through the project's `repositoryId`.
// GitHub itself stays storage-free (App JWT → JIT installation token on demand).
export const repositories = pgTable(
  `repositories`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
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
    unique().on(table.workspaceId, table.fullName),
    index(`idx_repositories_workspace`).on(table.workspaceId),
  ]
)

// Per-project terminal run commands (SERVER-ONLY, tRPC-managed — never an
// Electric shape; the proxy count stays 14). A run config is just a named
// argv the desktop apps spawn into a terminal tab (run configs live in the
// DATABASE, not the repo). SECURITY: because this is DB-stored argv
// executed locally, desktops MUST keep the per-device Trust & Run
// commandSetHash prompt and re-hash whenever the fetched config set changes —
// never auto-run synced values. `workspace_id` is denormalized from the
// project server-side on insert (tRPC-only writes, so no trigger needed).
export const runConfigs = pgTable(
  `run_configs`,
  {
    id: uuidPk(),
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
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
    unique().on(table.projectId, table.name),
    index(`idx_run_configs_workspace`).on(table.workspaceId),
  ]
)

// Per-user notification delivery prefs (SERVER-ONLY). Missing row = all
// defaults (email on, no digest). Email is a free delivery channel, never a
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
  // off|daily|weekly — documented varchar (server-only logic, no native picker).
  digest: varchar({ length: 16 }).notNull().default(`off`),
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
    // queued|sent|failed — documented varchar.
    status: varchar({ length: 16 }).notNull().default(`queued`),
    provider: varchar({ length: 16 }), // resend|smtp
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

// Embeddable feedback-widget configs (server-only, NOT Electric-synced; read
// via the `widgets` tRPC router). One row = one paste-in snippet: a public
// key scoped to a destination workspace+project, plus the domain allowlist
// that gates cross-origin submissions.
export const widgetConfigs = pgTable(
  `widget_configs`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    projectId: uuid(`project_id`)
      .notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    // `expw_` + 32 base62 chars. Public by design (it ships inside the host
    // page's snippet); the domain allowlist + rate limiting are the controls,
    // so it is stored in plaintext for direct lookup.
    publicKey: varchar(`public_key`, { length: 64 }).notNull().unique(),
    // Hostname[:port] patterns; `*.example.com` matches subdomains only.
    // Empty array = any origin may use the key (settings UI warns about this).
    allowedDomains: jsonb(`allowed_domains`)
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    enabled: boolean().notNull().default(true),
    // Appearance/behavior overrides served to the widget loader:
    // { buttonLabel?, accentColor?, position?, emailRequired? }.
    formConfig: jsonb(`form_config`).$type<Record<string, unknown>>(),
    // Synthetic bot user (users.isAgent) that owns issues created through this
    // widget — the ONE system user the v2 cuts preserve (unrelated to the
    // deleted desktop-agent identity). `restrict` because issues.creator_id
    // cascades on user delete — deleting this user would silently delete every
    // issue the widget ever created. Config deletion keeps the user around.
    widgetUserId: text(`widget_user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `restrict` }),
    createdByUserId: text(`created_by_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    ...timestamps,
  },
  (table) => [index(`idx_widget_configs_workspace`).on(table.workspaceId)]
)

// One row per issue created through a widget (server-only, NOT synced). The
// structured reporter contact + page/env context that must survive description
// edits; the issue's description carries the same data as a readable block.
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
    ...timestamps,
  },
  (table) => [
    index(`idx_widget_submissions_config`).on(table.widgetConfigId),
  ]
)

// What an OAuth-authenticated MCP client may touch (SERVER-ONLY, written by
// the /auth/consent page). One row per (user, oauth client); re-consenting
// replaces the selection. `workspaceIds` grants whole workspaces (including
// projects created later); `projectIds` grants individual projects. A token
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
    allWorkspaces: boolean(`all_workspaces`).notNull().default(false),
    workspaceIds: jsonb(`workspace_ids`)
      .$type<string[]>()
      .notNull()
      .default(sql`'[]'::jsonb`),
    projectIds: jsonb(`project_ids`)
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

export const selectWorkspaceSchema = createSelectSchema(workspaces)
export const createWorkspaceSchema = createInsertSchema(workspaces).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectWorkspaceMemberSchema = createSelectSchema(
  workspaceMembers,
  {
    role: workspaceRoleSchema,
  }
)
export const selectWorkspaceInviteSchema = createSelectSchema(
  workspaceInvites,
  {
    role: workspaceRoleSchema,
  }
)
export const selectProjectSchema = createSelectSchema(projects, {
  type: projectTypeSchema,
  publicShowCoding: publicCodingVisibilitySchema,
})
export const createProjectSchema = createInsertSchema(projects).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectIssueSchema = createSelectSchema(issues, {
  description: issueDescriptionSchema.nullable(),
  priority: issuePrioritySchema,
  status: issueStatusSchema,
  recurrenceUnit: recurrenceUnitSchema.nullable(),
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

export const selectReleaseSchema = createSelectSchema(releases, {
  prState: prStateSchema.nullable(),
})
export const createReleaseSchema = createInsertSchema(releases).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
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

export type Workspace = InferSelectModel<typeof workspaces>
export type WorkspaceMember = InferSelectModel<typeof workspaceMembers>
export type WorkspaceInvite = InferSelectModel<typeof workspaceInvites>
export type Project = InferSelectModel<typeof projects>
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
export type Release = InferSelectModel<typeof releases>
export type Repository = InferSelectModel<typeof repositories>
export type RunConfig = InferSelectModel<typeof runConfigs>
export type UserNotificationPrefs = InferSelectModel<
  typeof userNotificationPrefs
>
export type EmailDelivery = InferSelectModel<typeof emailDeliveries>
export type WidgetConfig = InferSelectModel<typeof widgetConfigs>
export type WidgetSubmission = InferSelectModel<typeof widgetSubmissions>
export type McpGrant = InferSelectModel<typeof mcpGrants>

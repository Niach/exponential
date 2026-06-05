import {
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
  uuid,
  varchar,
  boolean,
  date,
} from "drizzle-orm/pg-core"
import { sql, type InferSelectModel } from "drizzle-orm"
import { createSchemaFactory } from "drizzle-zod"
import { z } from "zod"
import {
  commentBodySchema,
  issueDescriptionSchema,
  issueEventTypeSchema,
  issuePrioritySchema,
  issuePriorityValues,
  issueStatusSchema,
  issueStatusValues,
  publicWritePolicyValues,
  recurrenceUnitSchema,
  recurrenceUnitValues,
  subscriberSourceSchema,
  workspaceRoleSchema,
  workspaceRoleValues,
} from "./domain"

export * from "./auth-schema"
import { apikeys, users } from "./auth-schema"

const { createInsertSchema, createSelectSchema } = createSchemaFactory({
  zodInstance: z,
})

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

export const issueStatusEnum = pgEnum(`issue_status`, issueStatusValues)

export const issuePriorityEnum = pgEnum(`issue_priority`, issuePriorityValues)

export const issueRelationTypeEnum = pgEnum(`issue_relation_type`, [
  `blocks`,
  `is_blocked_by`,
  `relates_to`,
  `duplicates`,
  `is_duplicated_by`,
])

export const notificationTypeEnum = pgEnum(`notification_type`, [
  `issue_assigned`,
  `issue_comment`,
  `issue_status_changed`,
  `issue_mention`,
])

export const workspaceMemberRoleEnum = pgEnum(
  `workspace_member_role`,
  workspaceRoleValues
)

export const publicWritePolicyEnum = pgEnum(
  `public_write_policy`,
  publicWritePolicyValues
)

export const recurrenceUnitEnum = pgEnum(
  `recurrence_unit`,
  recurrenceUnitValues
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

export const workspaces = pgTable(`workspaces`, {
  id: uuidPk(),
  name: varchar({ length: 255 }).notNull(),
  slug: varchar({ length: 255 }).notNull().unique(),
  iconUrl: text(`icon_url`),
  isPublic: boolean(`is_public`).notNull().default(false),
  publicWritePolicy: publicWritePolicyEnum(`public_write_policy`)
    .notNull()
    .default(`members`),
  ...timestamps,
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

export const workspaceAgents = pgTable(
  `workspace_agents`,
  {
    id: uuidPk(),
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // The human owner who registered this agent (D2: an agent is a distinct
    // actor owned by a human). Nullable during the cutover; tightened to
    // NOT NULL once all agents have re-registered via OAuth.
    ownerUserId: text(`owner_user_id`).references(() => users.id, {
      onDelete: `cascade`,
    }),
    name: varchar({ length: 255 }).notNull(),
    // Legacy setup-token columns (the old curl|bash claimSetup flow). Kept
    // nullable for backward inspection; no longer written by `companion.register`.
    setupTokenHash: text(`setup_token_hash`),
    setupTokenExpiresAt: timestamp(`setup_token_expires_at`, {
      withTimezone: true,
    }),
    setupTokenConsumedAt: timestamp(`setup_token_consumed_at`, {
      withTimezone: true,
    }),
    apiKeyId: text(`api_key_id`).references(() => apikeys.id, {
      onDelete: `set null`,
    }),
    // The per-agent OAuth client (oauth_applications.client_id) whose
    // access/refresh token pair is the agent's runtime credential.
    oauthClientId: text(`oauth_client_id`),
    lastSeenAt: timestamp(`last_seen_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    unique().on(table.workspaceId, table.userId),
    index(`idx_workspace_agents_workspace`).on(table.workspaceId),
    index(`idx_workspace_agents_user`).on(table.userId),
    index(`idx_workspace_agents_owner`).on(table.ownerUserId),
    index(`idx_workspace_agents_setup_token`).on(table.setupTokenHash),
    index(`idx_workspace_agents_api_key`).on(table.apiKeyId),
  ]
)

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
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    archivedAt: timestamp(`archived_at`, { withTimezone: true }),
    // GitHub repo this project is linked to, in `owner/name` form. null
    // means an agent assigned an issue here will mark it needs_human until
    // an owner links a repo from the workspace settings UI.
    githubRepo: text(`github_repo`),
    ...timestamps,
  },
  (table) => [unique().on(table.workspaceId, table.slug)]
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
    description: jsonb(),
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
    googleCalendarEventId: varchar(`google_calendar_event_id`, {
      length: 1024,
    }),
    googleCalendarLastSyncedAt: timestamp(`google_calendar_last_synced_at`, {
      withTimezone: true,
    }),
    googleCalendarLastSyncError: text(`google_calendar_last_sync_error`),
    agentPlanState: varchar(`agent_plan_state`, { length: 32 }),
    agentPlanRevision: integer(`agent_plan_revision`).notNull().default(0),
    agentPlanApprovedAt: timestamp(`agent_plan_approved_at`, {
      withTimezone: true,
    }),
    agentPlanApprovedBy: text(`agent_plan_approved_by`).references(
      () => users.id,
      { onDelete: `set null` }
    ),
    agentLastCommentSeenAt: timestamp(`agent_last_comment_seen_at`, {
      withTimezone: true,
    }),
    // PR linkage (D5: one issue = one PR = one branch/worktree). Written by the
    // agent via `agentPlan.reportPr` / the `exponential_agent_report_pr` MCP
    // tool, synced to every client so the diff view + PR badge work without
    // parsing comment bodies. All nullable (no PR until the agent opens one).
    prUrl: text(`pr_url`),
    prNumber: integer(`pr_number`),
    prState: varchar(`pr_state`, { length: 16 }),
    branch: text(`branch`),
    prMergedAt: timestamp(`pr_merged_at`, { withTimezone: true }),
    // Interactive-run bookkeeping (D4). `agentSessionId` is the claude session
    // to `--continue`; `agentRunMode` is background|interactive;
    // `agentInteractiveClaimedAt` marks that a desktop interactive session owns
    // the issue, which suppresses the automatic background code re-entry.
    agentSessionId: text(`agent_session_id`),
    agentRunMode: varchar(`agent_run_mode`, { length: 16 }),
    agentInteractiveClaimedAt: timestamp(`agent_interactive_claimed_at`, {
      withTimezone: true,
    }),
    ...timestamps,
  },
  (table) => [
    index(`idx_issues_project_status`).on(table.projectId, table.status),
    index(`idx_issues_assignee`).on(table.assigneeId),
    index(`idx_issues_due_date`).on(table.dueDate),
  ]
)

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
  },
  (table) => [
    primaryKey({ columns: [table.issueId, table.labelId] }),
    index(`idx_issue_labels_label`).on(table.labelId),
    index(`idx_issue_labels_workspace`).on(table.workspaceId),
  ]
)

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
    ...timestamps,
  },
  (table) => [unique().on(table.issueId, table.relatedIssueId, table.type)]
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
    authorId: text(`author_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    body: jsonb().notNull(),
    kind: varchar({ length: 16 }).notNull().default(`regular`),
    answeredAt: timestamp(`answered_at`, { withTimezone: true }),
    editedAt: timestamp(`edited_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [
    index(`idx_comments_issue`).on(table.issueId),
    index(`idx_comments_workspace`).on(table.workspaceId),
  ]
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
  ]
)

export const pushSubscriptions = pgTable(`push_subscriptions`, {
  id: uuidPk(),
  userId: text(`user_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  endpoint: text().notNull(),
  p256dh: text().notNull(),
  auth: text().notNull(),
  userAgent: text(`user_agent`),
  ...timestamps,
})

export const fcmTokens = pgTable(`fcm_tokens`, {
  id: uuidPk(),
  userId: text(`user_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  token: text().notNull().unique(),
  platform: varchar({ length: 20 }).notNull(),
  ...timestamps,
})

// GitHub App installations (server-only, not synced). Captured by the install
// setup route so the UI can show "installed" per user; token resolution itself
// is storage-free (the App JWT looks up a repo's installation on demand).
export const githubInstallations = pgTable(`github_installations`, {
  id: uuidPk(),
  installationId: bigint(`installation_id`, { mode: `number` })
    .notNull()
    .unique(),
  accountLogin: text(`account_login`),
  accountType: varchar(`account_type`, { length: 20 }),
  userId: text(`user_id`).references(() => users.id, { onDelete: `set null` }),
  ...timestamps,
})

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
    ...timestamps,
  },
  (table) => [
    index(`idx_notifications_user_unread`).on(table.userId, table.readAt),
  ]
)

// Who is subscribed to an issue (D7). Auto-populated on create/assign/comment/
// mention; a `manual` row with `unsubscribed=true` suppresses auto-resubscribe.
// Drives both the inbox feed and the notification push fan-out.
export const issueSubscribers = pgTable(
  `issue_subscribers`,
  {
    id: uuidPk(),
    issueId: uuid(`issue_id`)
      .notNull()
      .references(() => issues.id, { onDelete: `cascade` }),
    userId: text(`user_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    // Denormalized from issue→project by populate_issue_subscriber_workspace_id
    // so the Electric shape filter stays workspace-scoped (stable, no 409 churn).
    workspaceId: uuid(`workspace_id`)
      .notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    source: varchar({ length: 16 }).notNull(),
    unsubscribed: boolean().notNull().default(false),
    ...timestamps,
  },
  (table) => [
    unique().on(table.issueId, table.userId),
    index(`idx_issue_subscribers_user`).on(table.userId),
    index(`idx_issue_subscribers_workspace`).on(table.workspaceId),
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
    actorUserId: text(`actor_user_id`).references(() => users.id, {
      onDelete: `set null`,
    }),
    type: varchar({ length: 32 }).notNull(),
    payload: jsonb(),
    ...timestamps,
  },
  (table) => [
    index(`idx_issue_events_issue`).on(table.issueId),
    index(`idx_issue_events_workspace`).on(table.workspaceId),
  ]
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
export const selectWorkspaceAgentSchema = createSelectSchema(workspaceAgents)

export const selectProjectSchema = createSelectSchema(projects)
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

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type Workspace = InferSelectModel<typeof workspaces>
export type WorkspaceMember = InferSelectModel<typeof workspaceMembers>
export type WorkspaceInvite = InferSelectModel<typeof workspaceInvites>
export type WorkspaceAgent = InferSelectModel<typeof workspaceAgents>
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

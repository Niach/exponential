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
  issueDescriptionSchema,
  issuePrioritySchema,
  issuePriorityValues,
  issueStatusSchema,
  issueStatusValues,
  recurrenceUnitSchema,
  recurrenceUnitValues,
  workspaceRoleSchema,
  workspaceRoleValues,
} from "./domain"

export * from "./auth-schema"
import { users } from "./auth-schema"

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
    googleCalendarEventId: varchar(`google_calendar_event_id`, { length: 1024 }),
    googleCalendarLastSyncedAt: timestamp(`google_calendar_last_synced_at`, {
      withTimezone: true,
    }),
    googleCalendarLastSyncError: text(`google_calendar_last_sync_error`),
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
    authorId: text(`author_id`)
      .notNull()
      .references(() => users.id, { onDelete: `cascade` }),
    body: jsonb().notNull(),
    editedAt: timestamp(`edited_at`, { withTimezone: true }),
    ...timestamps,
  },
  (table) => [index(`idx_comments_issue`).on(table.issueId)]
)

export const attachments = pgTable(
  `attachments`,
  {
    id: uuidPk(),
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
    ...timestamps,
  },
  (table) => [index(`idx_attachments_issue`).on(table.issueId)]
)

export const views = pgTable(`views`, {
  id: uuidPk(),
  workspaceId: uuid(`workspace_id`)
    .notNull()
    .references(() => workspaces.id, { onDelete: `cascade` }),
  creatorId: text(`creator_id`)
    .notNull()
    .references(() => users.id, { onDelete: `cascade` }),
  name: varchar({ length: 255 }).notNull(),
  icon: varchar({ length: 50 }),
  filters: jsonb()
    .notNull()
    .default(sql`'[]'::jsonb`),
  sortBy: jsonb(`sort_by`)
    .notNull()
    .default(sql`'[]'::jsonb`),
  isShared: boolean(`is_shared`).notNull().default(false),
  sortOrder: doublePrecision(`sort_order`).notNull().default(0),
  ...timestamps,
})

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

export const selectCommentSchema = createSelectSchema(comments)
export const createCommentSchema = createInsertSchema(comments).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectAttachmentSchema = createSelectSchema(attachments)

export const selectViewSchema = createSelectSchema(views)
export const createViewSchema = createInsertSchema(views).omit({
  id: true,
  createdAt: true,
  updatedAt: true,
})

export const selectNotificationSchema = createSelectSchema(notifications)

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
export type View = InferSelectModel<typeof views>

export type User = InferSelectModel<typeof users>
export type Notification = InferSelectModel<typeof notifications>

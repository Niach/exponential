import { createCollection } from "@tanstack/react-db"
import { electricCollectionOptions } from "@tanstack/electric-db-collection"
import { snakeCamelMapper } from "@electric-sql/client"
import {
  selectAttachmentSchema,
  selectCodingSessionSchema,
  selectCommentSchema,
  selectIssueEventSchema,
  selectIssueLabelSchema,
  selectIssueSchema,
  selectIssueSubscriberSchema,
  selectLabelSchema,
  selectNotificationSchema,
  selectBoardSchema,
  selectUserSchema,
  selectTeamInviteSchema,
  selectTeamMemberSchema,
  selectTeamSchema,
} from "@/db/schema"

const baseUrl =
  typeof window !== `undefined`
    ? window.location.origin
    : `http://localhost:5173`

const shapeParser = {
  timestamp: (date: string) => new Date(date),
  timestamptz: (date: string) => new Date(date),
}

const columnMapper = snakeCamelMapper()

function getShapeUrl(path: string) {
  return new URL(path, baseUrl).toString()
}

export const teamCollection = createCollection(
  electricCollectionOptions({
    id: `teams`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/teams`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectTeamSchema,
    getKey: (item) => item.id,
  })
)

export const teamMemberCollection = createCollection(
  electricCollectionOptions({
    id: `team_members`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/team-members`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectTeamMemberSchema,
    getKey: (item) => item.id,
  })
)

export const boardCollection = createCollection(
  electricCollectionOptions({
    id: `boards`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/boards`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectBoardSchema,
    getKey: (item) => item.id,
  })
)

export const issueCollection = createCollection(
  electricCollectionOptions({
    id: `issues`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/issues`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectIssueSchema,
    getKey: (item) => item.id,
  })
)

export const labelCollection = createCollection(
  electricCollectionOptions({
    id: `labels`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/labels`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectLabelSchema,
    getKey: (item) => item.id,
  })
)

export const issueLabelCollection = createCollection(
  electricCollectionOptions({
    id: `issue_labels`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/issue-labels`),
      columnMapper,
    },
    schema: selectIssueLabelSchema,
    getKey: (item) => `${item.issueId}:${item.labelId}`,
  })
)

export const teamInviteCollection = createCollection(
  electricCollectionOptions({
    id: `team_invites`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/team-invites`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectTeamInviteSchema,
    getKey: (item) => item.id,
  })
)

export const userCollection = createCollection(
  electricCollectionOptions({
    id: `users`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/users`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectUserSchema,
    getKey: (item) => item.id,
  })
)

export const commentCollection = createCollection(
  electricCollectionOptions({
    id: `comments`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/comments`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectCommentSchema,
    getKey: (item) => item.id,
  })
)

// Synced so embedded images can reserve their intrinsic aspect-ratio (width/
// height) before the bytes load — eliminating layout shift on reload. Mirrors
// the attachments shape the mobile/native clients already sync.
export const attachmentCollection = createCollection(
  electricCollectionOptions({
    id: `attachments`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/attachments`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectAttachmentSchema,
    getKey: (item) => item.id,
  })
)

// Per-user inbox feed (notifications scoped to the signed-in user).
export const notificationCollection = createCollection(
  electricCollectionOptions({
    id: `notifications`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/notifications`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectNotificationSchema,
    getKey: (item) => item.id,
  })
)

// Activity-log timeline events, team-scoped.
export const issueEventCollection = createCollection(
  electricCollectionOptions({
    id: `issue_events`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/issue-events`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectIssueEventSchema,
    getKey: (item) => item.id,
  })
)

// Subscription rows, for the per-issue subscribe toggle's live state.
export const issueSubscriberCollection = createCollection(
  electricCollectionOptions({
    id: `issue_subscribers`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/issue-subscribers`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectIssueSubscriberSchema,
    getKey: (item) => item.id,
  })
)

// Live "coding now" sessions, team-scoped. Synced so every coordination
// client can render the coding-session badge + Watch/Steer button straight from
// sync (one row per interactive desktop session).
export const codingSessionCollection = createCollection(
  electricCollectionOptions({
    id: `coding_sessions`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/coding-sessions`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectCodingSessionSchema,
    getKey: (item) => item.id,
  })
)


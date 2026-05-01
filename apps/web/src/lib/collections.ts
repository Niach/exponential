import { createCollection } from "@tanstack/react-db"
import { electricCollectionOptions } from "@tanstack/electric-db-collection"
import { snakeCamelMapper } from "@electric-sql/client"
import {
  selectIssueLabelSchema,
  selectIssueSchema,
  selectLabelSchema,
  selectProjectSchema,
  selectUserSchema,
  selectWorkspaceInviteSchema,
  selectWorkspaceMemberSchema,
  selectWorkspaceSchema,
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

export const workspaceCollection = createCollection(
  electricCollectionOptions({
    id: `workspaces`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/workspaces`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectWorkspaceSchema,
    getKey: (item) => item.id,
  })
)

export const workspaceMemberCollection = createCollection(
  electricCollectionOptions({
    id: `workspace_members`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/workspace-members`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectWorkspaceMemberSchema,
    getKey: (item) => item.id,
  })
)

export const projectCollection = createCollection(
  electricCollectionOptions({
    id: `projects`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/projects`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectProjectSchema,
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

export const workspaceInviteCollection = createCollection(
  electricCollectionOptions({
    id: `workspace_invites`,
    shapeOptions: {
      url: getShapeUrl(`/api/shapes/workspace-invites`),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectWorkspaceInviteSchema,
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

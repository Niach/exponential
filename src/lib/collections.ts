import { createCollection } from "@tanstack/react-db"
import { electricCollectionOptions } from "@tanstack/electric-db-collection"
import { snakeCamelMapper } from "@electric-sql/client"
import {
  selectWorkspaceSchema,
  selectWorkspaceMemberSchema,
  selectWorkspaceInviteSchema,
  selectProjectSchema,
  selectIssueSchema,
  selectLabelSchema,
  selectIssueLabelSchema,
  selectUserSchema,
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

export const workspaceCollection = createCollection(
  electricCollectionOptions({
    id: `workspaces`,
    shapeOptions: {
      url: new URL(`/api/shapes/workspaces`, baseUrl).toString(),
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
      url: new URL(`/api/shapes/workspace-members`, baseUrl).toString(),
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
      url: new URL(`/api/shapes/projects`, baseUrl).toString(),
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
      url: new URL(`/api/shapes/issues`, baseUrl).toString(),
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
      url: new URL(`/api/shapes/labels`, baseUrl).toString(),
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
      url: new URL(`/api/shapes/issue-labels`, baseUrl).toString(),
      parser: shapeParser,
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
      url: new URL(`/api/shapes/workspace-invites`, baseUrl).toString(),
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
      url: new URL(`/api/shapes/users`, baseUrl).toString(),
      parser: shapeParser,
      columnMapper,
    },
    schema: selectUserSchema,
    getKey: (item) => item.id,
  })
)

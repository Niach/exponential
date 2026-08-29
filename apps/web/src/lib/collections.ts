import { createCollection } from "@tanstack/react-db"
import { electricCollectionOptions } from "@tanstack/electric-db-collection"
import { snakeCamelMapper } from "@electric-sql/client"
import {
  reportTransportFailure,
  reportTransportSuccess,
} from "@/lib/connectivity"
import {
  selectSyncedActionSchema,
  selectAutomationSchema,
  selectAttachmentSchema,
  selectCodingSessionSchema,
  selectCommentSchema,
  selectDeviceSchema,
  selectSyncedDeviceWorktreeSchema,
  selectIssueEventSchema,
  selectIssueLabelSchema,
  selectIssueSchema,
  selectIssueSubscriberSchema,
  selectIssueStatusRowSchema,
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

// EXP-533: the continuous connectivity signal. `fetchClient` is forwarded
// verbatim into `ShapeStream` and sits UNDERNEATH its infinite backoff
// wrapper, so this sees EVERY attempt including the live long-polls — which
// is what makes it a heartbeat rather than a one-shot. (`onError` is no use
// here: the stream swallows network errors into its retry loop and never
// calls it.) Any Response at all proves the server answered; only a thrown
// error counts against reachability, and an aborted request is not an outage.
// (Typed structurally and asserted at the use site: `typeof fetch` here
// resolves to Bun's, which carries a `preconnect` a plain wrapper has no
// business implementing.)
const shapeFetch = async (
  input: RequestInfo | URL,
  init?: RequestInit
): Promise<Response> => {
  try {
    const response = await fetch(input, init)
    reportTransportSuccess()
    return response
  } catch (error) {
    if (!(error instanceof Error) || error.name !== `AbortError`) {
      reportTransportFailure(error)
    }
    throw error
  }
}

// The per-collection shape wiring, identical for all 20 of them: the proxy URL,
// the timestamp parser and the snake→camel mapper `useLiveQuery` where clauses
// depend on.
function shapeOptions(path: string) {
  return {
    url: getShapeUrl(path),
    parser: shapeParser,
    columnMapper,
    fetchClient: shapeFetch as typeof fetch,
  }
}

export const teamCollection = createCollection(
  electricCollectionOptions({
    id: `teams`,
    shapeOptions: shapeOptions(`/api/shapes/teams`),
    schema: selectTeamSchema,
    getKey: (item) => item.id,
  })
)

export const teamMemberCollection = createCollection(
  electricCollectionOptions({
    id: `team_members`,
    shapeOptions: shapeOptions(`/api/shapes/team-members`),
    schema: selectTeamMemberSchema,
    getKey: (item) => item.id,
  })
)

export const boardCollection = createCollection(
  electricCollectionOptions({
    id: `boards`,
    shapeOptions: shapeOptions(`/api/shapes/boards`),
    schema: selectBoardSchema,
    getKey: (item) => item.id,
  })
)

export const issueCollection = createCollection(
  electricCollectionOptions({
    id: `issues`,
    shapeOptions: shapeOptions(`/api/shapes/issues`),
    schema: selectIssueSchema,
    getKey: (item) => item.id,
  })
)

export const labelCollection = createCollection(
  electricCollectionOptions({
    id: `labels`,
    shapeOptions: shapeOptions(`/api/shapes/labels`),
    schema: selectLabelSchema,
    getKey: (item) => item.id,
  })
)

// EXP-314 — per-team issue statuses (the 16th shape); team-scoped like
// labels. Builtin rows carry builtin_key; customs are member-managed.
export const issueStatusCollection = createCollection(
  electricCollectionOptions({
    id: `issue_statuses`,
    shapeOptions: shapeOptions(`/api/shapes/issue-statuses`),
    schema: selectIssueStatusRowSchema,
    getKey: (item) => item.id,
  })
)

// The actions shape excludes `body` server-side (EXP-268) — the synced row is
// the list projection; editors/runs fetch the body via tRPC `actions.get`.
export const actionCollection = createCollection(
  electricCollectionOptions({
    id: `actions`,
    shapeOptions: shapeOptions(`/api/shapes/actions`),
    schema: selectSyncedActionSchema,
    getKey: (item) => item.id,
  })
)

// EXP-583: automations are their own shape — a schedule/event trigger that
// runs an action on a device. The bound device's host filters this same shape
// to its own enabled rows.
export const automationCollection = createCollection(
  electricCollectionOptions({
    id: `automations`,
    shapeOptions: shapeOptions(`/api/shapes/automations`),
    schema: selectAutomationSchema,
    getKey: (item) => item.id,
  })
)

export const issueLabelCollection = createCollection(
  electricCollectionOptions({
    id: `issue_labels`,
    shapeOptions: shapeOptions(`/api/shapes/issue-labels`),
    schema: selectIssueLabelSchema,
    getKey: (item) => `${item.issueId}:${item.labelId}`,
  })
)

export const teamInviteCollection = createCollection(
  electricCollectionOptions({
    id: `team_invites`,
    shapeOptions: shapeOptions(`/api/shapes/team-invites`),
    schema: selectTeamInviteSchema,
    getKey: (item) => item.id,
  })
)

export const userCollection = createCollection(
  electricCollectionOptions({
    id: `users`,
    shapeOptions: shapeOptions(`/api/shapes/users`),
    schema: selectUserSchema,
    getKey: (item) => item.id,
  })
)

export const commentCollection = createCollection(
  electricCollectionOptions({
    id: `comments`,
    shapeOptions: shapeOptions(`/api/shapes/comments`),
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
    shapeOptions: shapeOptions(`/api/shapes/attachments`),
    schema: selectAttachmentSchema,
    getKey: (item) => item.id,
  })
)

// Per-user inbox feed (notifications scoped to the signed-in user).
export const notificationCollection = createCollection(
  electricCollectionOptions({
    id: `notifications`,
    shapeOptions: shapeOptions(`/api/shapes/notifications`),
    schema: selectNotificationSchema,
    getKey: (item) => item.id,
  })
)

// Activity-log timeline events, team-scoped.
export const issueEventCollection = createCollection(
  electricCollectionOptions({
    id: `issue_events`,
    shapeOptions: shapeOptions(`/api/shapes/issue-events`),
    schema: selectIssueEventSchema,
    getKey: (item) => item.id,
  })
)

// Subscription rows, for the per-issue subscribe toggle's live state.
export const issueSubscriberCollection = createCollection(
  electricCollectionOptions({
    id: `issue_subscribers`,
    shapeOptions: shapeOptions(`/api/shapes/issue-subscribers`),
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
    shapeOptions: shapeOptions(`/api/shapes/coding-sessions`),
    schema: selectCodingSessionSchema,
    getKey: (item) => item.id,
  })
)

// EXP-481: the per-user device registry (own rows + team-shared server rows).
// Online-ness derives client-side from last_seen_at freshness — no relay
// presence in the sync path.
export const deviceCollection = createCollection(
  electricCollectionOptions({
    id: `devices`,
    shapeOptions: shapeOptions(`/api/shapes/devices`),
    schema: selectDeviceSchema,
    getKey: (item) => item.id,
  })
)

// EXP-481: per-device worktree inventory — resume offers + the
// device-settings worktree list, from persisted data even while the device
// is offline.
export const deviceWorktreeCollection = createCollection(
  electricCollectionOptions({
    id: `device_worktrees`,
    shapeOptions: shapeOptions(`/api/shapes/device-worktrees`),
    schema: selectSyncedDeviceWorktreeSchema,
    getKey: (item) => item.id,
  })
)


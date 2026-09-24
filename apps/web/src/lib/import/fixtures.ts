// EXP-630: shared test fixtures — a small Linear snapshot shaped like the
// Methode 5 workspace (two teams, one project, three custom states, a
// duplicate pair, an upload in a description and a threaded comment) and
// the bundle it produces under team routing.
import type { ImportBundle, ImportPreview } from "@/lib/import/bundle"
import type { LinearSnapshot } from "@/lib/import/linear/snapshot"
import { linearPreview } from "@/lib/import/linear/bundle"
import type { TeamState } from "@/lib/import/plan"

export const T_MET = `team-met`
export const T_SOV = `team-sov`
export const U_DENNIS = `user-dennis`
export const U_HANNES = `user-hannes`
export const U_BOT = `user-bot`
export const P_MAINT = `project-maint`
export const UPLOAD_URL = `https://uploads.linear.app/org/aaaa/bbbb`
export const UPLOAD_URL_2 = `https://uploads.linear.app/org/cccc/dddd`

export function linearSnapshotFixture(): LinearSnapshot {
  return {
    version: 1,
    fetchedAt: `2026-09-23T12:00:00.000Z`,
    organization: { id: `org`, name: `Methode 5`, urlKey: `methode5` },
    viewer: { id: U_DENNIS, name: `Dennis`, email: `dennis@straehhuber.com` },
    teams: [
      { id: T_MET, key: `MET`, name: `Methode 5`, estimationType: `tShirt` },
      { id: T_SOV, key: `SOV`, name: `soverius-ai` },
    ],
    states: [
      { id: `st-backlog`, name: `Backlog`, type: `backlog`, color: `#bec2c8`, position: 0, teamId: T_MET },
      { id: `st-todo`, name: `Todo`, type: `unstarted`, color: `#e2e2e2`, position: 1, teamId: T_MET },
      { id: `st-icebox`, name: `Icebox`, type: `unstarted`, color: `#95a2b3`, position: 2, teamId: T_MET },
      { id: `st-progress`, name: `In Progress`, type: `started`, color: `#f2c94c`, position: 3, teamId: T_MET },
      { id: `st-rueck`, name: `Rückfrage`, type: `started`, color: `#f2994a`, position: 4, teamId: T_MET },
      { id: `st-review`, name: `In Review`, type: `started`, color: `#0f7488`, position: 5, teamId: T_MET },
      { id: `st-done`, name: `Done`, type: `completed`, color: `#5e6ad2`, position: 6, teamId: T_MET },
      { id: `st-canceled`, name: `Canceled`, type: `canceled`, color: `#95a2b3`, position: 7, teamId: T_MET },
      { id: `st-nicht`, name: `Nicht reproduzierbar`, type: `canceled`, color: `#95a2b3`, position: 8, teamId: T_MET },
      { id: `st-dup`, name: `Duplicate`, type: `duplicate`, color: `#95a2b3`, position: 9, teamId: T_MET },
      { id: `st-sov-backlog`, name: `Backlog`, type: `backlog`, color: `#bec2c8`, position: 0, teamId: T_SOV },
    ],
    labels: [
      { id: `lb-bug`, name: `Bug`, color: `#eb5757`, teamId: null, parentId: null },
      { id: `lb-group`, name: `Area`, color: `#4ea7fc`, teamId: T_MET, parentId: null },
      { id: `lb-ios`, name: `ios`, color: `#4ea7fc`, teamId: T_MET, parentId: `lb-group` },
    ],
    users: [
      { id: U_DENNIS, name: `dennis@straehhuber.com`, displayName: `dennis`, email: `dennis@straehhuber.com`, active: true },
      { id: U_HANNES, name: `Hannes Robier`, displayName: `hannes`, email: `hannes.robier@youspi.com`, active: true },
      { id: U_BOT, name: `Linear`, displayName: `linear`, email: `linear-org@linear.linear.app`, active: true },
    ],
    projects: [{ id: P_MAINT, name: `M5 - Maintenance`, teamIds: [T_MET] }],
    issues: [
      {
        id: `is-1`,
        teamId: T_MET,
        identifier: `MET-1`,
        number: 1,
        title: `First`,
        description: `Hello ![shot.png](${UPLOAD_URL}) and [file.pdf](${UPLOAD_URL_2})`,
        priority: 1,
        createdAt: `2025-01-01T10:00:00.000Z`,
        updatedAt: `2025-06-01T10:00:00.000Z`,
        completedAt: `2025-06-01T10:00:00.000Z`,
        canceledAt: null,
        archivedAt: null,
        dueDate: null,
        estimate: 3,
        stateId: `st-done`,
        assigneeId: U_HANNES,
        creatorId: U_DENNIS,
        projectId: P_MAINT,
        parentId: null,
        labelIds: [`lb-bug`, `lb-ios`],
        history: [
          {
            id: `h-1`,
            createdAt: `2025-03-01T10:00:00.000Z`,
            actorId: U_HANNES,
            fromStateId: `st-todo`,
            toStateId: `st-progress`,
            fromAssigneeId: null,
            toAssigneeId: U_HANNES,
            fromPriority: null,
            toPriority: null,
            addedLabelIds: [`lb-bug`],
            removedLabelIds: null,
          },
          {
            id: `h-2`,
            createdAt: `2025-06-01T10:00:00.000Z`,
            actorId: U_DENNIS,
            fromStateId: `st-progress`,
            toStateId: `st-done`,
            fromAssigneeId: null,
            toAssigneeId: null,
            fromPriority: 3,
            toPriority: 1,
            addedLabelIds: null,
            removedLabelIds: null,
          },
        ],
      },
      {
        id: `is-2`,
        teamId: T_MET,
        identifier: `MET-2`,
        number: 2,
        title: `Second (duplicate of first)`,
        description: null,
        priority: 0,
        createdAt: `2025-02-01T10:00:00.000Z`,
        updatedAt: `2025-02-02T10:00:00.000Z`,
        completedAt: null,
        canceledAt: `2025-02-02T10:00:00.000Z`,
        archivedAt: null,
        dueDate: `2025-03-01`,
        estimate: null,
        stateId: `st-dup`,
        assigneeId: null,
        creatorId: U_BOT,
        projectId: null,
        parentId: null,
        labelIds: [],
        history: [],
      },
      {
        id: `is-3`,
        teamId: T_MET,
        identifier: `MET-3`,
        number: 3,
        title: `Third (icebox, blocks first)`,
        description: `Plain`,
        priority: 4,
        createdAt: `2025-03-01T10:00:00.000Z`,
        updatedAt: `2025-03-01T10:00:00.000Z`,
        completedAt: null,
        canceledAt: null,
        archivedAt: null,
        dueDate: null,
        estimate: null,
        stateId: `st-icebox`,
        assigneeId: U_DENNIS,
        creatorId: U_HANNES,
        projectId: P_MAINT,
        parentId: `is-1`,
        labelIds: [],
        history: [],
      },
      {
        id: `is-4`,
        teamId: T_MET,
        identifier: `MET-4`,
        number: 4,
        title: `Fourth (archived, similar to first)`,
        description: null,
        priority: 2,
        createdAt: `2024-06-01T10:00:00.000Z`,
        updatedAt: `2024-07-01T10:00:00.000Z`,
        completedAt: `2024-07-01T10:00:00.000Z`,
        canceledAt: null,
        archivedAt: `2024-10-01T10:00:00.000Z`,
        dueDate: null,
        estimate: 2.4,
        stateId: `st-done`,
        assigneeId: null,
        creatorId: U_DENNIS,
        projectId: P_MAINT,
        parentId: null,
        labelIds: [`lb-bug`],
        history: [],
      },
    ],
    comments: [
      {
        id: `cm-1`,
        issueId: `is-1`,
        body: `Root comment`,
        createdAt: `2025-01-02T10:00:00.000Z`,
        updatedAt: `2025-01-02T10:00:00.000Z`,
        editedAt: null,
        userId: U_HANNES,
        parentId: null,
      },
      {
        id: `cm-2`,
        issueId: `is-1`,
        body: `Reply with ![pic](${UPLOAD_URL})`,
        createdAt: `2025-01-03T10:00:00.000Z`,
        updatedAt: `2025-01-04T10:00:00.000Z`,
        editedAt: `2025-01-04T10:00:00.000Z`,
        userId: U_DENNIS,
        parentId: `cm-1`,
      },
      {
        id: `cm-3`,
        issueId: `is-1`,
        body: `Reply to the reply`,
        createdAt: `2025-01-05T10:00:00.000Z`,
        updatedAt: null,
        editedAt: null,
        userId: null,
        parentId: `cm-2`,
      },
    ],
    relations: [
      { id: `rel-1`, type: `duplicate`, issueId: `is-2`, relatedIssueId: `is-1` },
      { id: `rel-2`, type: `blocks`, issueId: `is-3`, relatedIssueId: `is-1` },
      { id: `rel-3`, type: `related`, issueId: `is-1`, relatedIssueId: `is-3` },
      { id: `rel-4`, type: `similar`, issueId: `is-4`, relatedIssueId: `is-1` },
    ],
    assetSizes: { [UPLOAD_URL]: 85_285 },
  }
}

export const BUILTIN_ROWS: TeamState[`statuses`] = [
  { id: `s-backlog`, name: `Backlog`, category: `backlog`, builtinKey: `backlog`, color: `#a1a1aa` },
  { id: `s-progress`, name: `In Progress`, category: `started`, builtinKey: `in_progress`, color: `#eab308` },
  { id: `s-review`, name: `In Review`, category: `started`, builtinKey: `in_review`, color: `#22c55e` },
  { id: `s-done`, name: `Done`, category: `completed`, builtinKey: `done`, color: `#3b82f6` },
  { id: `s-cancelled`, name: `Cancelled`, category: `cancelled`, builtinKey: `cancelled`, color: `#a1a1aa` },
  { id: `s-duplicate`, name: `Duplicate`, category: `duplicate`, builtinKey: `duplicate`, color: `#a1a1aa` },
]

export const ME = `me-user-id`

export function teamStateFixture(overrides: Partial<TeamState> = {}): TeamState {
  return {
    teamId: `team-1`,
    boards: [],
    statuses: [...BUILTIN_ROWS],
    labels: [{ id: `label-bug`, name: `bug`, color: `#ef4444` }],
    members: [
      { userId: ME, email: `danny@straehhuber.com`, name: `Danny` },
      { userId: `member-hannes`, email: `HANNES.ROBIER@youspi.com`, name: `Hannes` },
    ],
    pendingInviteEmails: [],
    canInvite: true,
    storage: { limitBytes: null, usedBytes: 0 },
    importedIssueKeys: new Set(),
    importedBoards: new Map(),
    estimationType: `none`,
    ...overrides,
  }
}

// A minimal hand-written bundle for the applier tests (no Linear involved).
export function bundleFixture(): ImportBundle {
  return {
    version: 1,
    source: `test`,
    sourceLabel: `Test Tracker`,
    estimation: `tshirt`,
    boards: [{ key: `b-main`, name: `Main`, prefix: `MAIN` }],
    statuses: [
      { key: `st-open`, category: `backlog`, name: `Backlog`, color: `#aaaaaa` },
      { key: `st-doing`, category: `started`, name: `Doing`, color: `#f2c94c` },
      { key: `st-done`, category: `completed`, name: `Done`, color: `#5e6ad2` },
      { key: `st-dup`, category: `duplicate`, name: `Duplicate`, color: `#95a2b3` },
    ],
    labels: [
      { key: `lb-bug`, name: `Bug`, color: `#eb5757` },
      { key: `lb-new`, name: `Brand new`, color: `#4ea7fc` },
    ],
    users: [
      { key: `u-h`, name: `Hannes`, email: `hannes.robier@youspi.com` },
      { key: `u-x`, name: `Stranger`, email: `stranger@example.com` },
    ],
    issues: [
      {
        key: `i-1`,
        boardKey: `b-main`,
        number: 10,
        title: `Ten`,
        description: `See ![a](https://files.example.com/a.png)`,
        externalRef: `MAIN-10`,
        statusKey: `st-done`,
        priority: `high`,
        assigneeKey: `u-h`,
        creatorKey: `u-x`,
        createdAt: `2025-01-01T00:00:00.000Z`,
        updatedAt: `2025-02-01T00:00:00.000Z`,
        completedAt: `2025-02-01T00:00:00.000Z`,
        labelKeys: [`lb-bug`, `lb-new`],
        relatedKeys: [],
        blocksKeys: [],
        comments: [
          {
            key: `c-1`,
            authorKey: `u-x`,
            body: `From a stranger`,
            createdAt: `2025-01-02T00:00:00.000Z`,
          },
          {
            key: `c-2`,
            authorKey: `u-h`,
            body: `Reply`,
            createdAt: `2025-01-03T00:00:00.000Z`,
            parentKey: `c-1`,
          },
        ],
        events: [
          {
            key: `e-1`,
            type: `status_changed`,
            actorKey: `u-h`,
            createdAt: `2025-01-15T00:00:00.000Z`,
            fromStatusKey: `st-open`,
            toStatusKey: `st-doing`,
          },
        ],
        assets: [{ key: `a-1`, ref: `https://files.example.com/a.png`, filename: `a.png`, sizeBytes: 100 }],
      },
      {
        key: `i-2`,
        boardKey: `b-main`,
        number: 11,
        title: `Eleven`,
        externalRef: `MAIN-11`,
        statusKey: `st-dup`,
        priority: `none`,
        createdAt: `2025-01-05T00:00:00.000Z`,
        updatedAt: `2025-01-06T00:00:00.000Z`,
        completedAt: `2025-01-06T00:00:00.000Z`,
        labelKeys: [],
        duplicateOfKey: `i-1`,
        relatedKeys: [],
        blocksKeys: [],
        comments: [],
        events: [],
        assets: [],
      },
      {
        key: `i-3`,
        boardKey: `b-main`,
        number: 5,
        title: `Five (older number, blocks ten)`,
        externalRef: `MAIN-5`,
        statusKey: `st-doing`,
        priority: `low`,
        createdAt: `2025-01-07T00:00:00.000Z`,
        updatedAt: `2025-01-07T00:00:00.000Z`,
        estimate: 3,
        labelKeys: [],
        parentKey: `i-1`,
        relatedKeys: [`i-1`],
        blocksKeys: [`i-1`],
        comments: [],
        events: [],
        assets: [],
      },
    ],
  }
}

// The preview discovery would produce for the snapshot above.
export function previewFixture(): ImportPreview {
  return linearPreview(linearSnapshotFixture())
}

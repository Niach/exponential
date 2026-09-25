import { describe, expect, it } from "vitest"
import { getTableColumns } from "drizzle-orm"
import {
  codingSessions,
  issues,
  workflowEvents,
  workflowNodes,
  workflows,
} from "@exp/db-schema"

import { ISSUE_COLUMNS } from "./issues"
import { WORKFLOW_COLUMNS } from "./workflows"
import { WORKFLOW_NODE_COLUMNS } from "./workflow-nodes"
import { WORKFLOW_EVENT_COLUMNS } from "./workflow-events"
import { CODING_SESSION_COLUMNS } from "./coding-sessions"

// A proxy's `columns` allowlist is sent to Electric verbatim: a name the table
// does not have fails the SHAPE, not the column — the client then syncs none
// of that table and the surface renders empty with nothing in the log. Dropping
// a column without dropping it here is the whole failure mode, so the lists are
// checked against the schema rather than against a copy of themselves.
const lists = {
  issues: [ISSUE_COLUMNS, issues],
  workflows: [WORKFLOW_COLUMNS, workflows],
  workflow_nodes: [WORKFLOW_NODE_COLUMNS, workflowNodes],
  // EXP-1082: the new membership columns and the event log.
  workflow_events: [WORKFLOW_EVENT_COLUMNS, workflowEvents],
  coding_sessions: [CODING_SESSION_COLUMNS, codingSessions],
} as const

describe(`shape column allowlists`, () => {
  it.each(Object.entries(lists))(`%s names only real columns`, (_name, [columns, table]) => {
    const real = new Set(
      Object.values(getTableColumns(table)).map((column) => column.name)
    )
    expect([...columns].filter((column) => !real.has(column))).toEqual([])
  })
})

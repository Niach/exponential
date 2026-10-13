import { describe, expect, it } from "vitest"
import { getTableColumns } from "drizzle-orm"
import {
  issueDrafts,
  issues,
  messages,
  pullRequestIssues,
  pullRequests,
  sessionIssues,
  sessions,
} from "@exp/db-schema"

import { ISSUE_COLUMNS } from "./issues"
import { ISSUE_DRAFT_COLUMNS } from "./issue-drafts"
import { SESSION_COLUMNS } from "./sessions"
import { SESSION_ISSUE_COLUMNS } from "./session-issues"
import { MESSAGE_COLUMNS } from "./messages"
import { PULL_REQUEST_COLUMNS } from "./pull-requests"
import { PULL_REQUEST_ISSUE_COLUMNS } from "./pull-request-issues"

// A proxy's `columns` allowlist is sent to Electric verbatim: a name the table
// does not have fails the SHAPE, not the column — the client then syncs none
// of that table and the surface renders empty with nothing in the log. Dropping
// a column without dropping it here is the whole failure mode, so the lists are
// checked against the schema rather than against a copy of themselves.
const lists = {
  issues: [ISSUE_COLUMNS, issues],
  issue_drafts: [ISSUE_DRAFT_COLUMNS, issueDrafts],
  // EXP-1291
  sessions: [SESSION_COLUMNS, sessions],
  session_issues: [SESSION_ISSUE_COLUMNS, sessionIssues],
  messages: [MESSAGE_COLUMNS, messages],
  pull_requests: [PULL_REQUEST_COLUMNS, pullRequests],
  pull_request_issues: [PULL_REQUEST_ISSUE_COLUMNS, pullRequestIssues],
} as const

describe(`shape column allowlists`, () => {
  it.each(Object.entries(lists))(`%s names only real columns`, (_name, [columns, table]) => {
    const real = new Set(
      Object.values(getTableColumns(table)).map((column) => column.name)
    )
    expect([...columns].filter((column) => !real.has(column))).toEqual([])
  })
})

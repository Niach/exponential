import { describe, expect, it } from "vitest"
import { getTableColumns } from "drizzle-orm"
import {
  agents,
  events,
  issueDrafts,
  issues,
  messages,
  prompts,
  pullRequestIssues,
  pullRequests,
  sessionIssues,
  sessionReads,
  sessions,
} from "@exp/db-schema"

import { ISSUE_COLUMNS } from "./issues"
import { ISSUE_DRAFT_COLUMNS } from "./issue-drafts"
import { SESSION_COLUMNS } from "./sessions"
import { SESSION_ISSUE_COLUMNS } from "./session-issues"
import { MESSAGE_COLUMNS } from "./messages"
import { SESSION_READ_COLUMNS } from "./session-reads"
import { PULL_REQUEST_COLUMNS } from "./pull-requests"
import { PULL_REQUEST_ISSUE_COLUMNS } from "./pull-request-issues"
import { AGENT_COLUMNS } from "./agents"
import { EVENT_COLUMNS } from "./events"
import { PROMPT_COLUMNS } from "./prompts"

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
  // EXP-1323
  session_reads: [SESSION_READ_COLUMNS, sessionReads],
  // NEW-2
  agents: [AGENT_COLUMNS, agents],
  events: [EVENT_COLUMNS, events],
  prompts: [PROMPT_COLUMNS, prompts],
} as const

describe(`shape column allowlists`, () => {
  it.each(Object.entries(lists))(`%s names only real columns`, (_name, [columns, table]) => {
    const real = new Set(
      Object.values(getTableColumns(table)).map((column) => column.name)
    )
    expect([...columns].filter((column) => !real.has(column))).toEqual([])
  })
})

describe(`sessions allowlist`, () => {
  it(`keeps the legacy read shims for the shipped clients, never the server-only columns`, () => {
    for (const shim of [
      `dispatch`,
      `caption`,
      `aside`,
      `move_to_device_id`,
      `handoff_restored_at`,
      `spawned_by_session_id`,
    ]) {
      expect(SESSION_COLUMNS).toContain(shim)
    }
    expect(SESSION_COLUMNS).not.toContain(`wake_on`)
    expect(SESSION_COLUMNS).not.toContain(`mcp_server_ids`)
    // NEW-2: the agent facets moved to `agents`; their shims stay listed.
    for (const shim of [`kind`, `name`, `icon`, `color`, `triggers`, `sort_order`, `action_id`, `action_name`, `action_inputs`]) {
      expect(SESSION_COLUMNS).toContain(shim)
    }
    expect(SESSION_COLUMNS).toContain(`agent_id`)
  })
  it(`keeps an agent's instructions and trigger bookkeeping server-only`, () => {
    expect(AGENT_COLUMNS).not.toContain(`instructions`)
    expect(AGENT_COLUMNS).not.toContain(`trigger_state`)
    expect(PROMPT_COLUMNS).not.toContain(`body`)
  })
  it(`keeps the never-written assignment column the shipped desktop names`, () => {
    expect(SESSION_ISSUE_COLUMNS).toContain(`assigned_session_id`)
  })
})

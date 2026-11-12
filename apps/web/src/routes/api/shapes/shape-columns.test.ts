import { describe, expect, it } from "vitest"
import { getTableColumns } from "drizzle-orm"
import {
  agents,
  appVersions,
  appRecords,
  apps,
  comments,
  issueEvents,
  conversations,
  events,
  issueDrafts,
  composerDrafts,
  issues,
  messages,
  pins,
  prompts,
  skills,
  pullRequestChecks,
  pullRequestIssues,
  pullRequests,
  sessionIssues,
  sessionReads,
  sessions,
} from "@exp/db-schema"

import { ISSUE_COLUMNS } from "./issues"
import { ISSUE_DRAFT_COLUMNS } from "./issue-drafts"
import { COMPOSER_DRAFT_COLUMNS } from "./composer-drafts"
import { SESSION_COLUMNS } from "./sessions"
import { SESSION_ISSUE_COLUMNS } from "./session-issues"
import { MESSAGE_COLUMNS } from "./messages"
import { SESSION_READ_COLUMNS } from "./session-reads"
import { PULL_REQUEST_COLUMNS } from "./pull-requests"
import { PULL_REQUEST_ISSUE_COLUMNS } from "./pull-request-issues"
import { PULL_REQUEST_CHECK_COLUMNS } from "./pull-request-checks"
import { AGENT_COLUMNS } from "./agents"
import { EVENT_COLUMNS } from "./events"
import { PROMPT_COLUMNS } from "./prompts"
import { SKILL_COLUMNS } from "./skills"
import { CONVERSATION_COLUMNS } from "./conversations"
import { APP_COLUMNS } from "./apps"
import { APP_VERSION_COLUMNS } from "./app-versions"
import { COMMENT_COLUMNS } from "./comments"
import { ISSUE_EVENT_COLUMNS } from "./issue-events"
import { APP_RECORD_COLUMNS } from "@/routes/api/shapes/app-records"

// A proxy's `columns` allowlist is sent to Electric verbatim: a name the table
// does not have fails the SHAPE, not the column — the client then syncs none
// of that table and the surface renders empty with nothing in the log. Dropping
// a column without dropping it here is the whole failure mode, so the lists are
// checked against the schema rather than against a copy of themselves.
const lists = {
  issues: [ISSUE_COLUMNS, issues],
  issue_drafts: [ISSUE_DRAFT_COLUMNS, issueDrafts],
  composer_drafts: [COMPOSER_DRAFT_COLUMNS, composerDrafts],
  // EXP-1291
  sessions: [SESSION_COLUMNS, sessions],
  session_issues: [SESSION_ISSUE_COLUMNS, sessionIssues],
  messages: [MESSAGE_COLUMNS, messages],
  pull_requests: [PULL_REQUEST_COLUMNS, pullRequests],
  pull_request_issues: [PULL_REQUEST_ISSUE_COLUMNS, pullRequestIssues],
  // EXP-1610
  pull_request_checks: [PULL_REQUEST_CHECK_COLUMNS, pullRequestChecks],
  // EXP-1323
  session_reads: [SESSION_READ_COLUMNS, sessionReads],
  // NEW-2
  agents: [AGENT_COLUMNS, agents],
  events: [EVENT_COLUMNS, events],
  prompts: [PROMPT_COLUMNS, prompts],
  // #EXP-1656
  skills: [SKILL_COLUMNS, skills],
  // NEW-3
  conversations: [CONVERSATION_COLUMNS, conversations],
  // VAPP-110
  apps: [APP_COLUMNS, apps],
  app_versions: [APP_VERSION_COLUMNS, appVersions],
  // #EXP-1383
  comments: [COMMENT_COLUMNS, comments],
  issue_events: [ISSUE_EVENT_COLUMNS, issueEvents],
  // VAPP-122
  app_records: [APP_RECORD_COLUMNS, appRecords],
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
  it(`keeps the bus columns server-only`, () => {
    expect(SESSION_COLUMNS).not.toContain(`wake_on`)
    expect(SESSION_COLUMNS).not.toContain(`mcp_server_ids`)
    expect(SESSION_COLUMNS).not.toContain(`skill_ids`)
    expect(SESSION_COLUMNS).toContain(`agent_id`)
  })
  it(`the NEW-8 legacy columns are gone from the tables`, () => {
    const names = (table: Parameters<typeof getTableColumns>[0]) =>
      Object.values(getTableColumns(table)).map((column) => column.name)
    for (const legacy of [
      `kind`,
      `dispatch`,
      `caption`,
      `aside`,
      `move_to_device_id`,
      `handoff_restored_at`,
      `spawned_by_session_id`,
      `name`,
      `icon`,
      `color`,
      `triggers`,
      `sort_order`,
      `action_id`,
      `action_name`,
      `action_inputs`,
    ]) {
      expect(names(sessions)).not.toContain(legacy)
    }
    expect(names(sessionIssues)).not.toContain(`assigned_session_id`)
    expect(names(pins)).not.toContain(`action_id`)
  })
  it(`keeps an agent's instructions and trigger bookkeeping server-only`, () => {
    expect(AGENT_COLUMNS).not.toContain(`instructions`)
    expect(AGENT_COLUMNS).not.toContain(`trigger_state`)
    expect(PROMPT_COLUMNS).not.toContain(`body`)
  })
  it(`#EXP-1656: a skill's body stays server-only, an agent's pick syncs`, () => {
    expect(SKILL_COLUMNS).not.toContain(`body`)
    expect(SKILL_COLUMNS).toContain(`description`)
    expect(SKILL_COLUMNS).toContain(`enabled_by_default`)
    expect(AGENT_COLUMNS).toContain(`skill_ids`)
    expect(AGENT_COLUMNS).toContain(`mcp_server_ids`)
  })
  it(`syncs the #EXP-1383 agent columns, never the instructions text`, () => {
    expect(AGENT_COLUMNS).toContain(`instructions_updated_at`)
    expect(SESSION_COLUMNS).toContain(`agent_instructions_at`)
    expect(SESSION_COLUMNS).toContain(`started_by_agent_id`)
    expect(ISSUE_COLUMNS).toEqual(expect.arrayContaining([`assignee_agent_id`, `creator_agent_id`]))
    expect(ISSUE_DRAFT_COLUMNS).toContain(`assignee_agent_id`)
    expect(COMMENT_COLUMNS).toContain(`author_agent_id`)
    expect(ISSUE_EVENT_COLUMNS).toContain(`actor_agent_id`)
    expect(ISSUE_EVENT_COLUMNS).not.toContain(`via_mcp`)
  })
  it(`syncs the VAPP-229 "via <app>" column on every row an app causes`, () => {
    for (const columns of [ISSUE_COLUMNS, SESSION_COLUMNS, APP_RECORD_COLUMNS, COMMENT_COLUMNS]) expect(columns).toContain(`via_app_id`)
  })
  it(`syncs the EXP-1458 tree key and sealed title`, () => {
    expect(SESSION_COLUMNS).toEqual(expect.arrayContaining([`seal_public_key`, `seal_epoch`, `sealed_title`, `seal_rotate_epoch`]))
  })
  it(`keeps the FEED-160 PR-state guard server-only`, () => {
    expect(PULL_REQUEST_COLUMNS).not.toContain(`github_updated_at`)
  })
  it(`syncs the EXP-1610 CI state, never the queued flag`, () => {
    expect(PULL_REQUEST_COLUMNS).toContain(`checks_state`)
    expect(PULL_REQUEST_CHECK_COLUMNS).not.toContain(`queued_flagged_at`)
  })
  it(`syncs the EXP-1709 merge train state, never its server-only clocks`, () => {
    expect(PULL_REQUEST_COLUMNS).toEqual(
      expect.arrayContaining([`merge_state`, `merge_wait`, `merge_hold`, `merge_position`, `merge_train_id`])
    )
    expect(PULL_REQUEST_COLUMNS).not.toContain(`merge_ready_at`)
    expect(PULL_REQUEST_COLUMNS).not.toContain(`merge_approved_at`)
  })
  it(`keeps a meeting message's expiry server-only (NEW-57)`, () => {
    expect(MESSAGE_COLUMNS).not.toContain(`expires_at`)
  })
  it(`keeps a send's idempotency key server-only (F21)`, () => {
    expect(MESSAGE_COLUMNS).not.toContain(`client_message_id`)
  })
})

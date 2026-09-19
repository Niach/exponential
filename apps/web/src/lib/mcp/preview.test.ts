import { describe, expect, it, vi } from "vitest"
import type { z } from "zod"

// EXP-988 contract table for `toolResultPreview` (lib/mcp/preview.ts).
//
// The ONE live assertion here is the drift gate: every registered
// `exponential_*` tool has a row in PREVIEW_KINDS, and no row names a tool
// that no longer registers. A tool added later fails this test until EXP-920
// maps it. The per-tool behaviour cases are skipped until EXP-920 lands the
// implementation and un-skips them.

vi.mock(`@/routes/api/trpc/$`, () => ({
  appRouter: { createCaller: vi.fn(() => ({})) },
}))
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  resolveTeamAccess: vi.fn(),
  assertTeamMember: vi.fn(),
  getIssueTeamContext: vi.fn(),
  getBoardTeamId: vi.fn(),
  getAttachmentTeamContext: vi.fn(),
  getUserTeamIds: vi.fn(),
  getPublicTeamIds: vi.fn(),
}))
vi.mock(`@/lib/storage`, () => ({
  uploadObject: vi.fn(),
  deleteObject: vi.fn(),
  getObject: vi.fn(),
}))
vi.mock(`@/lib/storage/image-dimensions`, () => ({
  getImageDimensions: vi.fn(),
}))
vi.mock(`@/lib/billing`, () => ({ assertWithinStorageLimit: vi.fn() }))
vi.mock(`@/lib/integrations/github-pr`, () => ({ createPullRequest: vi.fn() }))
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoInstallationToken: vi.fn(),
  resolveRepoInstallationTokenInfo: vi.fn(),
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrLifecycleStatusInTx: vi.fn(),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetPrNotify: vi.fn(),
}))
vi.mock(`@/lib/widget/agent-report`, () => ({
  createAgentBugReport: vi.fn(),
}))

// The widest surface: every gate open plus the cloud-only report_bug.
vi.stubEnv(`CLOUD_INSTANCE`, `true`)

import { registerExponentialTools } from "@/lib/mcp/tools"
import { FULL_ACCESS } from "@/lib/mcp/scope"
import { ALL_MCP_TOOL_GATES } from "@/lib/mcp/gates"
import type { McpUser } from "@/lib/mcp/server"
import { entityRefKinds, type EntityRefKind } from "@/lib/mcp/preview"

function registeredToolNames(): string[] {
  const names: string[] = []
  const fakeServer = {
    registerTool: (name: string, _def: { inputSchema?: z.ZodType }) => {
      names.push(name)
    },
  }
  registerExponentialTools(
    fakeServer as never,
    { id: `u` } as unknown as McpUser,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS,
    null,
    ALL_MCP_TOOL_GATES
  )
  return names.sort()
}

/** Tool → the ref kinds its result yields (in order). `[]` = no preview: the
 * row renders its contract caption alone. */
export const PREVIEW_KINDS: Record<string, EntityRefKind[]> = {
  exponential_actions_create: [`action`],
  exponential_actions_delete: [],
  exponential_actions_list: [`list`, `action`],
  exponential_actions_update: [`action`],
  exponential_attachments_delete: [],
  exponential_attachments_get: [],
  exponential_attachments_list: [`list`],
  exponential_attachments_upload: [],
  exponential_automations_create: [`automation`],
  exponential_automations_delete: [],
  exponential_automations_list: [`list`, `automation`],
  exponential_automations_toggle: [`automation`],
  exponential_automations_update: [`automation`],
  exponential_boards_create: [`board`],
  exponential_boards_delete: [],
  exponential_boards_get: [`board`],
  exponential_boards_list: [`list`, `board`],
  exponential_boards_set_repository: [`board`],
  exponential_boards_update: [`board`],
  exponential_comments_create: [`comment`, `issue`],
  exponential_comments_delete: [],
  exponential_comments_list: [`list`, `comment`],
  exponential_comments_update: [`comment`],
  exponential_devices_list: [`list`],
  exponential_helpdesk_close: [],
  exponential_helpdesk_escalate: [`issue`],
  exponential_helpdesk_note: [],
  exponential_helpdesk_reopen: [],
  exponential_helpdesk_reply: [],
  exponential_helpdesk_threads_get: [],
  exponential_helpdesk_threads_list: [`list`],
  exponential_invites_create: [],
  exponential_invites_list: [`list`],
  exponential_invites_revoke: [],
  exponential_issue_labels_add: [`issue`, `label`],
  exponential_issue_labels_remove: [`issue`, `label`],
  exponential_issue_relations_add: [`issue`, `issue`],
  exponential_issue_relations_remove: [`issue`, `issue`],
  exponential_issues_create: [`issue`],
  exponential_issues_delete: [],
  exponential_issues_get: [`issue`],
  exponential_issues_list: [`list`, `issue`],
  exponential_issues_pr_files: [`issue`],
  exponential_issues_subscribe: [`issue`],
  exponential_issues_unsubscribe: [`issue`],
  exponential_issues_update: [`issue`],
  exponential_issues_update_status: [`issue`, `status`],
  exponential_labels_create: [`label`],
  exponential_labels_delete: [],
  exponential_labels_get: [`label`],
  exponential_labels_list: [`list`, `label`],
  exponential_labels_update: [`label`],
  exponential_members_list: [`list`],
  exponential_notifications_list: [`list`, `issue`],
  exponential_notifications_mark_read: [],
  exponential_notifications_send: [],
  exponential_pr_merge: [`issue`],
  exponential_pr_open: [`issue`],
  exponential_pr_retarget: [`issue`],
  exponential_report_bug: [],
  exponential_repositories_add: [],
  exponential_repositories_branch_diff: [],
  exponential_repositories_list: [`list`],
  exponential_sessions_ask_parent: [],
  exponential_sessions_compact: [],
  exponential_sessions_end: [],
  exponential_sessions_get: [`session`, `issue`],
  exponential_sessions_kill: [`session`],
  exponential_sessions_list: [`list`, `session`],
  exponential_sessions_message: [`session`],
  exponential_sessions_results: [],
  exponential_sessions_start: [`session`, `issue`],
  exponential_statuses_create: [`status`],
  exponential_statuses_delete: [],
  exponential_statuses_list: [`list`, `status`],
  exponential_statuses_update: [`status`],
  exponential_teams_create: [],
  exponential_teams_get: [],
  exponential_teams_list: [`list`],
  exponential_teams_update: [],
  exponential_workflows_cancel: [],
  exponential_workflows_checkpoint: [],
  exponential_workflows_create: [`list`, `issue`],
  exponential_workflows_get: [`list`, `issue`],
  exponential_workflows_list: [`list`],
  exponential_workflows_pause: [],
  exponential_workflows_request_upstream: [`issue`],
  exponential_workflows_review_submit: [`issue`],
  exponential_workflows_start: [`list`, `issue`],
  exponential_workflows_update: [`list`, `issue`],
}

describe(`toolResultPreview covers the whole tool surface (EXP-988/EXP-920)`, () => {
  it(`lists every registered tool, and nothing else`, () => {
    expect(Object.keys(PREVIEW_KINDS).sort()).toEqual(registeredToolNames())
  })

  it(`names only contract ref kinds`, () => {
    for (const [name, kinds] of Object.entries(PREVIEW_KINDS)) {
      for (const kind of kinds) {
        expect(entityRefKinds, `${name}: ${kind}`).toContain(kind)
      }
    }
  })
})

describe.skip(`toolResultPreview (EXP-920)`, () => {
  it(`yields the kinds of PREVIEW_KINDS for a representative result of every tool`, () => {
    // For each PREVIEW_KINDS entry: feed a fixture result and expect
    // toolResultPreview(name, result).map((ref) => ref.kind) to equal it.
  })

  it(`returns [] for an error result and for an unknown tool`, () => {})

  it(`gives every ref an in-app href and, for lists, the count as title`, () => {})
})

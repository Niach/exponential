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
import {
  entityRefKinds,
  listRefNoun,
  toolResultPreview,
  type EntityRef,
  type EntityRefKind,
} from "@/lib/mcp/preview"
import { contract } from "@exp/domain-contract"
import fixture from "@exp/domain-contract/fixtures/tool-result-preview.json"

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

/** Tool → the ref kinds its result yields (in order; a `list` is followed
 *  by its member kind). `[]` = no preview: the row renders its contract
 *  caption alone. Derived by hand from contract `expToolPreview.tools` and
 *  locked against it below. */
export const PREVIEW_KINDS: Record<string, EntityRefKind[]> = {
  exponential_actions_create: [`action`],
  exponential_actions_delete: [],
  exponential_actions_list: [`list`, `action`],
  exponential_actions_update: [`action`],
  exponential_attachments_delete: [],
  exponential_attachments_get: [`attachment`],
  exponential_attachments_list: [`list`, `attachment`, `issue`],
  exponential_attachments_upload: [`attachment`],
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
  exponential_comments_list: [`list`, `comment`, `issue`],
  exponential_comments_update: [`comment`, `issue`],
  exponential_devices_list: [`list`, `device`],
  exponential_helpdesk_close: [],
  exponential_helpdesk_escalate: [`issue`],
  exponential_helpdesk_note: [],
  exponential_helpdesk_reopen: [],
  exponential_helpdesk_reply: [],
  exponential_helpdesk_threads_get: [`thread`, `issue`],
  exponential_helpdesk_threads_list: [`list`, `thread`],
  exponential_invites_create: [`invite`],
  exponential_invites_list: [`list`, `invite`],
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
  exponential_members_list: [`list`, `member`],
  exponential_notifications_list: [`list`, `notification`],
  exponential_notifications_mark_read: [],
  exponential_notifications_send: [],
  exponential_pr_merge: [`issue`],
  exponential_pr_open: [`issue`, `issue`],
  exponential_pr_retarget: [`issue`],
  exponential_report_bug: [`issue`],
  exponential_repositories_add: [`repository`],
  exponential_repositories_branch_diff: [],
  exponential_repositories_list: [`list`, `repository`],
  exponential_sessions_ask_parent: [],
  exponential_sessions_compact: [],
  exponential_sessions_end: [],
  exponential_sessions_get: [`session`, `issue`],
  exponential_sessions_kill: [`session`],
  exponential_sessions_list: [`list`, `session`],
  exponential_sessions_message: [`session`],
  exponential_sessions_results: [],
  exponential_sessions_start: [`session`, `issue`, `issue`, `issue`],
  exponential_statuses_create: [`status`],
  exponential_statuses_delete: [],
  exponential_statuses_list: [`list`, `status`],
  exponential_statuses_update: [`status`],
  exponential_teams_create: [`team`],
  exponential_teams_get: [`team`],
  exponential_teams_list: [`list`, `team`],
  exponential_teams_update: [`team`],
  exponential_workflows_cancel: [`workflow`],
  exponential_workflows_checkpoint: [],
  exponential_workflows_create: [`workflow`, `issue`],
  exponential_workflows_get: [`workflow`, `list`, `issue`],
  exponential_workflows_list: [`list`, `workflow`],
  exponential_workflows_pause: [`workflow`],
  exponential_workflows_request_upstream: [`issue`],
  exponential_workflows_review_submit: [],
  exponential_workflows_start: [`workflow`],
  exponential_workflows_update: [`workflow`],
}

/** The kinds a contract spec (`kind@path …`) names, in order. */
function specKinds(refs: string): string[] {
  return refs
    .split(/\s+/)
    .filter(Boolean)
    .flatMap((term) => {
      const head = term.slice(0, term.indexOf(`@`))
      return head.startsWith(`list:`) ? [`list`, head.slice(5)] : [head]
    })
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

describe(`toolResultPreview (EXP-920)`, () => {
  it(`maps every registered tool in the contract spec table, and nothing else`, () => {
    expect(contract.expToolPreview.tools.map((tool) => `${contract.expToolDisplay.prefix}${tool.name}`).sort()).toEqual(
      registeredToolNames()
    )
  })

  it(`PREVIEW_KINDS is the kinds of the contract spec, tool by tool`, () => {
    for (const tool of contract.expToolPreview.tools) {
      const name = `${contract.expToolDisplay.prefix}${tool.name}`
      expect(specKinds(tool.refs), name).toEqual(PREVIEW_KINDS[name])
    }
  })

  it(`yields the fixture's refs for every case (locked ×2 with the engine)`, () => {
    expect(fixture.cases.length).toBeGreaterThan(40)
    for (const testCase of fixture.cases) {
      expect(
        toolResultPreview(testCase.tool, testCase.result, testCase.input),
        testCase.name
      ).toEqual(testCase.refs)
    }
  })

  it(`every fixture ref is well-formed: a contract kind, a non-empty id, strings within textMax, count only on lists`, () => {
    for (const testCase of fixture.cases) {
      for (const ref of testCase.refs as EntityRef[]) {
        expect(entityRefKinds, testCase.name).toContain(ref.kind)
        expect(ref.id.length, testCase.name).toBeGreaterThan(0)
        for (const text of [ref.id, ref.identifier, ref.title]) {
          if (text !== undefined) {
            expect(Array.from(text).length, testCase.name).toBeLessThanOrEqual(
              contract.expToolPreview.textMax
            )
          }
        }
        if (ref.kind === `list`) {
          expect(ref.count, testCase.name).toBeTypeOf(`number`)
          expect(entityRefKinds, testCase.name).toContain(ref.id)
        } else {
          expect(ref.count, testCase.name).toBeUndefined()
        }
      }
      expect(testCase.refs.length).toBeLessThanOrEqual(contract.expToolPreview.maxRefs)
    }
  })

  it(`returns [] for an error result and for an unknown tool`, () => {
    expect(
      toolResultPreview(`exponential_issues_create`, {
        isError: true,
        content: [{ type: `text`, text: `nope` }],
      })
    ).toEqual([])
    expect(toolResultPreview(`Read`, { id: `x` })).toEqual([])
    expect(toolResultPreview(`exponential_issues_create`, null)).toEqual([])
  })

  it(`reads the adapter-namespaced name like the bare one`, () => {
    const result = { id: `i-1`, identifier: `EXP-1`, title: `x` }
    expect(toolResultPreview(`mcp__exponential__exponential_issues_get`, result)).toEqual(
      toolResultPreview(`exponential_issues_get`, result)
    )
  })

  it(`gives a list ref the plural noun and the member kind as id`, () => {
    expect(listRefNoun(`status`)).toBe(`statuses`)
    expect(listRefNoun(`repository`)).toBe(`repositories`)
    expect(listRefNoun(`issue`)).toBe(`issues`)
    const [list] = toolResultPreview(`exponential_labels_list`, [{ id: `l`, name: `bug` }])
    expect(list).toEqual({ kind: `list`, id: `label`, title: `labels`, count: 1 })
  })
})

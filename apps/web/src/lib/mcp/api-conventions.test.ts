import { describe, expect, it, vi } from "vitest"
import { z } from "zod"

// EXP-705/EXP-707: drift gates for the API conventions (lib/api-conventions.ts).
// Every rule here exists because its violation once shipped silently — a tool
// added with a bare raw shape strips unknown keys, an unbounded list tool
// dumps a whole table, a hand-copied enum drifts from the contract.

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

// Measure the worst-case surface (cloud-only report_bug included).
vi.stubEnv(`CLOUD_INSTANCE`, `true`)

import { readFileSync } from "node:fs"
import { join } from "node:path"
import { contract } from "@exp/domain-contract"
import { customizableStatusCategoryValues } from "@exp/db-schema/domain"
import { registerExponentialTools } from "@/lib/mcp/tools"
import { FULL_ACCESS } from "@/lib/mcp/scope"
import { ALL_MCP_TOOL_GATES } from "@/lib/mcp/gates"
import { issueWireColumns } from "@/lib/issue-columns"
import type { McpUser } from "@/lib/mcp/server"

type ToolDef = { description?: string; inputSchema?: z.ZodType }

function collectDefs(): Map<string, ToolDef> {
  const defs = new Map<string, ToolDef>()
  const fakeServer = {
    registerTool: (name: string, def: ToolDef) => {
      defs.set(name, def)
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
  return defs
}

const defs = collectDefs()

function jsonSchema(def: ToolDef): Record<string, unknown> {
  return z.toJSONSchema(def.inputSchema ?? z.strictObject({}), {
    io: `input`,
    target: `draft-7`,
  }) as Record<string, unknown>
}

describe(`strict inputs (EXP-705)`, () => {
  it(`every tool rejects unknown keys and publishes additionalProperties:false`, () => {
    expect(defs.size).toBeGreaterThan(0)
    for (const [name, def] of defs) {
      const schema = jsonSchema(def)
      expect(schema.additionalProperties, name).toBe(false)
      const parsed = def.inputSchema?.safeParse({
        __exp_bogus_key__: true,
      })
      expect(parsed?.success, name).toBe(false)
    }
  })
})

function paramSchema(tool: string, param: string): z.ZodType {
  const def = defs.get(tool)
  expect(def, tool).toBeDefined()
  const shape = (def!.inputSchema as z.ZodObject<z.ZodRawShape>)
    .shape as Record<string, z.ZodType>
  expect(shape[param], `${tool}.${param}`).toBeDefined()
  return shape[param]
}

describe(`one pagination model (EXP-707 theme G)`, () => {
  it(`every *_list tool declares a limit`, () => {
    const listTools = [...defs.keys()].filter((name) => name.endsWith(`_list`))
    expect(listTools.length).toBeGreaterThan(10)
    for (const name of listTools) {
      const schema = jsonSchema(defs.get(name)!)
      const properties = schema.properties as Record<string, unknown>
      expect(properties?.limit, name).toBeDefined()
    }
  })
})

describe(`identifier acceptance (EXP-707 theme A)`, () => {
  // Every issue-taking param accepts a human identifier ("ABC-12"), not just
  // a UUID — the shared resolver (lib/issue-resolver.ts) maps it.
  const issueParams: Array<[string, string]> = [
    [`exponential_issues_get`, `id`],
    [`exponential_issues_update`, `id`],
    [`exponential_issues_delete`, `id`],
    [`exponential_issues_update_status`, `id`],
    [`exponential_comments_list`, `issueId`],
    [`exponential_comments_create`, `issueId`],
    [`exponential_issue_labels_add`, `issueId`],
    [`exponential_issue_labels_remove`, `issueId`],
    [`exponential_issue_relations_add`, `issueId`],
    [`exponential_issue_relations_add`, `relatedIssueId`],
    [`exponential_issue_relations_remove`, `issueId`],
    [`exponential_issue_relations_remove`, `relatedIssueId`],
    [`exponential_issues_subscribe`, `issueId`],
    [`exponential_issues_unsubscribe`, `issueId`],
    [`exponential_pr_open`, `issueId`],
    [`exponential_pr_merge`, `issueId`],
    [`exponential_pr_retarget`, `issueId`],
    [`exponential_issues_pr_files`, `issueId`],
    [`exponential_repositories_branch_diff`, `issueId`],
    [`exponential_sessions_start`, `issueId`],
    [`exponential_attachments_upload`, `issueId`],
  ]
  it.each(issueParams)(`%s accepts an identifier for %s`, (tool, param) => {
    expect(paramSchema(tool, param).safeParse(`ABC-12`).success).toBe(true)
  })

  it(`issueIds arrays accept identifiers too`, () => {
    for (const tool of [
      `exponential_pr_open`,
      `exponential_pr_merge`,
      `exponential_sessions_start`,
    ]) {
      expect(
        paramSchema(tool, `issueIds`).safeParse([`ABC-12`]).success,
        tool
      ).toBe(true)
    }
  })
})

describe(`no hand-copied enums (EXP-707 theme H)`, () => {
  it(`statuses_create.category matches the derived customizable list`, () => {
    const category = paramSchema(`exponential_statuses_create`, `category`)
    for (const value of customizableStatusCategoryValues) {
      expect(category.safeParse(value).success, value).toBe(true)
    }
    expect(category.safeParse(`duplicate`).success).toBe(false)
    expect(category.safeParse(`bogus`).success).toBe(false)
  })

  it(`automation agent params share the contract enum`, () => {
    for (const tool of [
      `exponential_automations_create`,
      `exponential_automations_update`,
      `exponential_sessions_start`,
    ]) {
      const agent = paramSchema(tool, `agent`)
      for (const value of contract.codingAgent.values) {
        expect(agent.safeParse(value).success, `${tool}:${value}`).toBe(true)
      }
      expect(agent.safeParse(`gemini`).success, tool).toBe(false)
    }
  })
})

describe(`envelope + projection drift (EXP-707 themes E/I)`, () => {
  const webSrc = join(__dirname, `..`, `..`)

  it(`no tRPC router returns lowercase txid`, () => {
    // The SQL alias inside lib/trpc.ts generateTxId is the one legit use.
    const dir = join(webSrc, `lib`, `trpc`)
    const files = readFileSync(join(dir, `automations.ts`), `utf8`)
      .concat(readFileSync(join(dir, `steer.ts`), `utf8`))
      .concat(readFileSync(join(dir, `issues.ts`), `utf8`))
      .concat(readFileSync(join(dir, `boards.ts`), `utf8`))
      .concat(readFileSync(join(dir, `actions.ts`), `utf8`))
    expect(/\btxid\b/.test(files)).toBe(false)
  })

  it(`the MCP issue projection mirrors the issues shape allowlist`, () => {
    const source = readFileSync(
      join(webSrc, `routes`, `api`, `shapes`, `issues.ts`),
      `utf8`
    )
    const match = source.match(
      /export const ISSUE_COLUMNS = \[([\s\S]*?)\n\]/
    )
    expect(match).not.toBeNull()
    const body = match![1]
      .split(`\n`)
      .filter((line) => !line.trim().startsWith(`//`))
      .join(`\n`)
    const snakeColumns = [...body.matchAll(/`([a-z0-9_]+)`/g)].map((m) => m[1])
    const camelToSnake = (name: string) =>
      name.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`)
    expect(Object.keys(issueWireColumns).map(camelToSnake)).toEqual(
      snakeColumns
    )
  })
})

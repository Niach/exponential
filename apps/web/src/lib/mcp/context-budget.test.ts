import { expect, it, vi } from "vitest"
import { existsSync, readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { z } from "zod"

// EXP-353/EXP-637: what the MCP tool surface costs an agent's context window.
//
// History: until EXP-637 every client loaded EVERY tool definition verbatim,
// so the whole surface shared one ~24.6k ceiling and each new tool was paid
// for by trimming another. That is over. Claude Code defers MCP tools behind
// tool search by default (only names + the server `instructions` load at
// session start, `_meta["anthropic/alwaysLoad"]` opts a tool back in), Codex
// does the same on gpt-5.4+, and the pi bridge mirrors the split through pi's
// dynamic tool loading off the same flag.
//
// So the budget that matters is the ALWAYS-LOADED set, which is what a coding
// run needs on its first turn, plus the instructions. The whole surface still
// gets a loose ceiling so nobody ships a novella, and per-tool stays tight:
// uuid params use the compact `uuidString` (never z.uuid()'s 155-char
// pattern), icon params never inline the 60-name enum, descriptions stay
// terse. Same story for CLAUDE.md — Claude Code warns above 40k chars.

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

// EXP-496: exponential_report_bug only registers on a cloud instance — stub
// the env so the budget measures the WORST-case tool surface, not the
// self-hosted subset.
vi.stubEnv(`CLOUD_INSTANCE`, `true`)

import { registerExponentialTools } from "@/lib/mcp/tools"
import { FULL_ACCESS } from "@/lib/mcp/scope"
import {
  ALWAYS_LOAD_TOOLS,
  GATED_ALWAYS_LOAD_TOOLS,
} from "@/lib/mcp/always-load"
import { ALL_MCP_TOOL_GATES } from "@/lib/mcp/gates"
import {
  MCP_SERVER_INSTRUCTIONS,
  mcpServerInstructions,
} from "@/lib/mcp/instructions"
import type { McpUser } from "@/lib/mcp/server"

type ToolDef = {
  description?: string
  inputSchema?: Record<string, z.ZodType>
  _meta?: Record<string, unknown>
}

function serializeToolDefs(gates = ALL_MCP_TOOL_GATES) {
  const defs: Array<Record<string, unknown>> = []
  const fakeServer = {
    registerTool: (name: string, def: ToolDef) => {
      // Mirror the MCP SDK's tools/list serialization (name + description +
      // JSON-schema'd input; draft-7 target like zod-json-schema-compat).
      defs.push({
        name,
        description: def.description,
        inputSchema: z.toJSONSchema(z.object(def.inputSchema ?? {}), {
          io: `input`,
          target: `draft-7`,
        }),
        ...(def._meta ? { _meta: def._meta } : {}),
      })
    },
  }
  registerExponentialTools(
    fakeServer as never,
    { id: `u` } as unknown as McpUser,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS,
    null,
    gates
  )
  return defs
}

it(`keeps the always-loaded MCP tool set exactly ALWAYS_LOAD_TOOLS`, () => {
  const defs = serializeToolDefs()
  // Guard the CLOUD_INSTANCE stub above: if the cloud-only tool ever stops
  // registering here, the budget silently under-measures.
  expect(defs.some((def) => def.name === `exponential_report_bug`)).toBe(true)
  // EXP-660: the helpdesk family registers behind a per-caller gate whose
  // DEFAULT is the full surface — if that default ever flips, the budget
  // would silently stop measuring seven tools.
  expect(
    defs.some((def) => def.name === `exponential_helpdesk_threads_list`)
  ).toBe(true)
  // Every listed tool actually registers, and nothing else carries the flag —
  // adding `_meta` to a tool is adding it to EVERY session's context, so it
  // has to be a deliberate edit of always-load.ts.
  const flagged = defs
    .filter(
      (def) =>
        (def._meta as Record<string, unknown> | undefined)?.[
          `anthropic/alwaysLoad`
        ] === true
    )
    .map((def) => def.name)
  expect([...flagged].sort()).toEqual(
    [...ALWAYS_LOAD_TOOLS, ...GATED_ALWAYS_LOAD_TOOLS].sort()
  )
  // exponential_report_bug is deliberately NOT always-loaded: an agent that
  // needs it searches for it.
  expect(flagged).not.toContain(`exponential_report_bug`)
})

// EXP-679: the close-out tool is registered only for an unattended run, so a
// person-started session must not even see the name.
it(`registers exponential_sessions_end only behind its gate`, () => {
  expect(
    serializeToolDefs().some((def) => def.name === `exponential_sessions_end`)
  ).toBe(true)
  const person = serializeToolDefs({
    helpdesk: true,
    sessionsEnd: false,
    askParent: false,
  })
  expect(person.some((def) => def.name === `exponential_sessions_end`)).toBe(
    false
  )
})

// EXP-700: the ask tool is registered only for an AGENT-started run with a
// linked parent; every other unattended run (schedule/event) stays without it.
it(`registers exponential_sessions_ask_parent only behind its gate`, () => {
  expect(
    serializeToolDefs().some(
      (def) => def.name === `exponential_sessions_ask_parent`
    )
  ).toBe(true)
  const automation = serializeToolDefs({
    helpdesk: true,
    sessionsEnd: true,
    askParent: false,
  })
  expect(
    automation.some((def) => def.name === `exponential_sessions_ask_parent`)
  ).toBe(false)
  // The generic message tool is NOT gated — a header-less orchestrator
  // answers its children with it.
  expect(
    automation.some((def) => def.name === `exponential_sessions_message`)
  ).toBe(true)
})

it(`keeps the serialized MCP tool context within budget`, () => {
  const defs = serializeToolDefs()
  // The gated one counts too: an unattended run carries it from turn one.
  const alwaysLoadedNames: readonly string[] = [
    ...ALWAYS_LOAD_TOOLS,
    ...GATED_ALWAYS_LOAD_TOOLS,
  ]
  const alwaysLoaded = defs.filter((def) =>
    alwaysLoadedNames.includes(def.name as string)
  )
  // What EVERY session pays, on every turn. Keep it lean — a tool added here
  // is a tool every agent carries whether it needs it or not.
  expect(JSON.stringify(alwaysLoaded).length).toBeLessThan(10_000)
  // The deferred remainder is fetched on demand, so the whole surface only
  // needs a sanity ceiling (it was 24.6k when everything loaded eagerly).
  expect(JSON.stringify(defs).length).toBeLessThan(60_000)
  for (const def of defs) {
    // No single tool may reintroduce a fat inline enum or a novella — a
    // searched-for definition still lands verbatim in the window.
    expect(JSON.stringify(def).length, `${def.name}`).toBeLessThan(1_800)
  }
})

// EXP-637: the server instructions are the ONLY guidance a deferred-tool
// client sees up front. Codex reads the first 512 chars, Claude Code
// truncates the whole string at 2KB — so the first paragraph has to stand on
// its own and name the way in.
it(`keeps the MCP server instructions self-contained and in budget`, () => {
  expect(MCP_SERVER_INSTRUCTIONS.length).toBeLessThan(2_000)
  const opening = MCP_SERVER_INSTRUCTIONS.slice(0, 512)
  expect(opening).toContain(`exponential_pr_open`)
  expect(opening).toContain(`Search for exponential_*`)
  // EXP-660: the deferred families an agent would never guess at by name.
  expect(opening).toContain(`helpdesk`)
  expect(opening).toContain(`sessions`)
  // The first paragraph must fit that window whole — a sentence cut in half
  // at 512 is worse than no sentence.
  expect(MCP_SERVER_INSTRUCTIONS.split(`\n\n`)[0].length).toBeLessThanOrEqual(
    512
  )
  // EXP-679: the person-started variant must not mention a tool it does not
  // get, and must stay inside the same two budgets.
  const person = mcpServerInstructions({
    sessionsEnd: false,
    askParent: false,
    reportBug: true,
  })
  expect(person).not.toContain(`exponential_sessions_end`)
  expect(MCP_SERVER_INSTRUCTIONS).toContain(`exponential_sessions_end`)
  expect(person.length).toBeLessThan(2_000)
  expect(person.split(`\n\n`)[0].length).toBeLessThanOrEqual(512)
  // EXP-700: same rule for the ask tool — an automation-started run (no
  // parent) must not be told to ask a starter it does not have.
  const automation = mcpServerInstructions({
    sessionsEnd: true,
    askParent: false,
    reportBug: true,
  })
  expect(automation).not.toContain(`exponential_sessions_ask_parent`)
  expect(MCP_SERVER_INSTRUCTIONS).toContain(`exponential_sessions_ask_parent`)
  // FEED-21: the report-bug trigger follows the tool's EXP-496 cloud gate — a
  // self-hosted instance never registers the tool, so it must not name it.
  const selfHosted = mcpServerInstructions({
    sessionsEnd: true,
    askParent: true,
    reportBug: false,
  })
  expect(selfHosted).not.toContain(`exponential_report_bug`)
  expect(MCP_SERVER_INSTRUCTIONS).toContain(`exponential_report_bug`)
})

it(`keeps CLAUDE.md under Claude Code's 40k-char performance warning`, () => {
  // Walk up from the vitest cwd (apps/web) to the repo root.
  let dir = process.cwd()
  while (!existsSync(join(dir, `CLAUDE.md`)) && dirname(dir) !== dir) {
    dir = dirname(dir)
  }
  const claudeMd = readFileSync(join(dir, `CLAUDE.md`), `utf8`)
  expect(claudeMd.length).toBeLessThan(40_000)
})

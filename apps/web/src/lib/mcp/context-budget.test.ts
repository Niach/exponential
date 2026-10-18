import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"
import { expect, it, vi } from "vitest"
import { existsSync, readFileSync } from "node:fs"
import { dirname, join } from "node:path"

// EXP-353/EXP-637: what the MCP tool surface costs an agent's context window.
//
// EXP-1051 moved the `tools/list` serializer itself to `lib/mcp/context-budget.ts`
// — the SAME numbers now ride `codingSessions.contextBudget` into a run's
// context bar, so the measurement and the budget cannot drift apart.
//
// History: until EXP-637 every client loaded EVERY tool definition verbatim,
// so the whole surface shared one ~24.6k ceiling and each new tool was paid
// for by trimming another. That is over. Claude Code defers MCP tools behind
// tool search by default (only names + the server `instructions` load at
// session start, `_meta["anthropic/alwaysLoad"]` opts a tool back in), Codex
// does the same on gpt-5.4+.
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
  // SLOP-7: lib/auth/index.ts reads the App's OAuth client at import time.
  githubOAuthClient: () => null,
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrLifecycleStatusInTx: vi.fn(),
}))
vi.mock(`@/lib/pr-stacks`, () => ({
  inferBaseBranch: vi.fn(async (opts: { defaultBranch: string }) => ({
    base: opts.defaultBranch,
    prNumber: null,
  })),
  ensureGithubStack: vi.fn(async () => ({ number: 1, baseRef: `master`, open: true, pulls: [] })),
  mergeThrough: vi.fn(),
}))
vi.mock(`@/lib/run-pr-body`, () => ({
  runPrBody: vi.fn(),
  runHasReportBody: vi.fn(),
  syncRunPrBody: vi.fn(),
  stampRunResultsPrUrl: vi.fn(async () => false),
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

import {
  mcpContextBudget,
  serializeToolDefs,
} from "@/lib/mcp/context-budget"
import {
  ALWAYS_LOAD_TOOLS,
  GATED_ALWAYS_LOAD_TOOLS,
} from "@/lib/mcp/always-load"
import {
  MCP_SERVER_INSTRUCTIONS,
  mcpServerInstructions,
} from "@/lib/mcp/instructions"

it(`keeps the always-loaded MCP tool set exactly ALWAYS_LOAD_TOOLS`, () => {
  const defs = serializeToolDefs()
  // Guard the CLOUD_INSTANCE stub above: if the cloud-only tool ever stops
  // registering here, the budget silently under-measures.
  expect(defs.some((def) => def.name === `exponential_report_bug`)).toBe(true)
  // EXP-660: the gated families register behind per-caller gates whose
  // DEFAULT is the full surface — if that default ever flips, the budget
  // would silently stop measuring them.
  expect(defs.some((def) => def.name === `exponential_guide_write`)).toBe(
    true
  )
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

// EXP-1291: the tools that act on the caller's OWN session (the Guide,
// compaction) register only behind the session gate; `sessions_end` takes an
// id, so every caller has it.
it(`registers the own-session tools only behind the session gate`, () => {
  const headerless = serializeToolDefs({
    inSession: false,
    unattended: false,
    agent: false,
  }).map((def) => def.name)
  const inSession = serializeToolDefs({
    inSession: true,
    unattended: false,
    agent: false,
  }).map((def) => def.name)
  for (const name of [`exponential_guide_write`, `exponential_sessions_compact`]) {
    expect(inSession, name).toContain(name)
    expect(headerless, name).not.toContain(name)
  }
  expect(headerless).toContain(`exponential_sessions_end`)
  expect(headerless).toContain(`exponential_sessions_send`)
  // EXP-1291: the retired surface is gone.
  for (const name of [
    `exponential_sessions_start`,
    `exponential_sessions_message`,
    `exponential_sessions_messages`,
    `exponential_sessions_ask_parent`,
    `exponential_sessions_guide`,
    `exponential_sessions_show`,
    `exponential_sessions_kill`,
    `exponential_notifications_send`,
    `exponential_threads_get`,
    `exponential_issues_pr_files`,
    `exponential_repositories_branch_diff`,
  ]) {
    expect(inSession, name).not.toContain(name)
  }
})

it(`keeps the serialized MCP tool context within budget`, () => {
  const defs = serializeToolDefs()
  // The gated one counts too: every session carries it from turn one.
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
  // EXP-1051: the shipped helper measures the same set, in UTF-8 BYTES (never
  // fewer than the JS string length, and within the same ceiling) — the
  // number `codingSessions.contextBudget` hands the launcher for the `tools`
  // segment of the context bar.
  const budget = mcpContextBudget()
  expect(budget.mcpAlwaysLoadBytes).toBeGreaterThanOrEqual(
    JSON.stringify(alwaysLoaded).length
  )
  expect(budget.mcpAlwaysLoadBytes).toBeLessThan(10_000)
  expect(budget.mcpInstructionsBytes).toBeGreaterThanOrEqual(
    MCP_SERVER_INSTRUCTIONS.length
  )
  expect(budget.mcpInstructionsBytes).toBeLessThan(2_000)
  // Memoized per process: the same object back, never a re-registration.
  expect(mcpContextBudget()).toBe(budget)
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
  expect(opening).toContain(`sessions`)
  expect(opening).toContain(`devices`)
  expect(opening).not.toContain(`helpdesk`)
  // The first paragraph must fit that window whole — a sentence cut in half
  // at 512 is worse than no sentence.
  expect(MCP_SERVER_INSTRUCTIONS.split(`\n\n`)[0].length).toBeLessThanOrEqual(
    512
  )
  // EXP-1222: the person-started variant has the close-out tool but is told
  // to call it only when asked — never the unattended LAST rule.
  const person = mcpServerInstructions({
    inSession: true,
    unattended: false,
    agent: false,
    reportBug: true,
  })
  expect(person).toContain(
    `exponential_sessions_end ends this session; call it only when the person asks you to end the session.`
  )
  expect(person).not.toContain(`This session is unattended`)
  // EXP-1280: only a planning session is told it is in plan mode (the
  // fixture's gate sentence); every session gets the ask.
  expect(MCP_SERVER_INSTRUCTIONS).toContain(PLAN_CARD_COPY.planModeGate)
  expect(person).not.toContain(PLAN_CARD_COPY.planModeGate)
  expect(person).toContain(`exponential_sessions_ask`)
  expect(MCP_SERVER_INSTRUCTIONS).toContain(`exponential_sessions_end LAST`)
  expect(MCP_SERVER_INSTRUCTIONS).not.toContain(`only when the person asks`)
  // A team agent never ends and writes no Guide of its own.
  const agent = mcpServerInstructions({
    inSession: true,
    unattended: false,
    agent: true,
    reportBug: true,
  })
  expect(agent).toContain(`You are a team agent`)
  expect(agent).not.toContain(`exponential_sessions_end`)
  // FEED-21: the report-bug trigger follows the tool's EXP-496 cloud gate.
  const selfHosted = mcpServerInstructions({
    inSession: true,
    unattended: true,
    agent: false,
    reportBug: false,
  })
  expect(selfHosted).not.toContain(`exponential_report_bug`)
  expect(MCP_SERVER_INSTRUCTIONS).toContain(`exponential_report_bug`)
  // EXP-879/1251: the Guide ask follows the session gate.
  const noSession = mcpServerInstructions({
    inSession: false,
    unattended: false,
    agent: false,
    reportBug: true,
  })
  expect(noSession).not.toContain(`Close out with a Guide`)
  expect(noSession).not.toContain(`exponential_sessions_end`)
  expect(MCP_SERVER_INSTRUCTIONS).toContain(`Close out with a Guide`)
  // EXP-1291: people are pinged by @email, never a notify tool.
  expect(MCP_SERVER_INSTRUCTIONS).toContain(`@email`)
  expect(MCP_SERVER_INSTRUCTIONS).not.toContain(`notifications_send`)
  for (const variant of [person, agent, selfHosted, noSession]) {
    expect(variant.length).toBeLessThan(2_000)
    expect(variant.split(`\n\n`)[0].length).toBeLessThanOrEqual(512)
  }
})

/** The repo root, walked up from the vitest cwd (apps/web). */
function repoRoot(): string {
  let dir = process.cwd()
  while (!existsSync(join(dir, `CLAUDE.md`)) && dirname(dir) !== dir) {
    dir = dirname(dir)
  }
  return dir
}

it(`keeps CLAUDE.md under Claude Code's 40k-char performance warning`, () => {
  const claudeMd = readFileSync(join(repoRoot(), `CLAUDE.md`), `utf8`)
  expect(claudeMd.length).toBeLessThan(40_000)
})

// EXP-763: the run playbook the desktop + CLI launcher appends to EVERY
// coding session's system prompt (claude `--append-system-prompt`, codex
// `developer_instructions`). It lives with the launcher
// (`crates/coding/src/runbook.md`, EXP-1291) because that is what ships it, but it names
// server tools by their exact names — so the server's test is what catches a
// renamed or removed tool, and the byte cap mirrors the crate's own.
it(`keeps the run playbook in budget and naming only registered tools`, () => {
  const playbook = readFileSync(
    join(repoRoot(), `apps/desktop/crates/coding/src/runbook.md`),
    `utf8`
  )
  expect(playbook.length).toBeLessThan(6 * 1024)
  const registered = new Set(serializeToolDefs().map((def) => def.name))
  const mentioned = new Set(
    [...playbook.matchAll(/exponential_[a-z_]+/g)].map((m) => m[0])
  )
  for (const name of mentioned) {
    expect(registered.has(name), `playbook names unregistered ${name}`).toBe(true)
  }
  // The point of the document: the deferred tools a run never found by itself.
  for (const name of [
    `exponential_sessions_send`,
    `exponential_issues_create`,
    `exponential_issue_relations_add`,
    `exponential_report_bug`,
    // EXP-988: the contract tools the leaves fill in.
    `exponential_attachments_list`,
    `exponential_attachments_upload`,
    `exponential_sessions_compact`,
    // EXP-1251 → EXP-1291: the close-out Guide's tool, and what the agent
    // links into its thread.
    `exponential_guide_write`,
    `exponential_sessions_get`,
    `exponential_sessions_link`,
  ]) {
    expect(mentioned.has(name), `playbook never names ${name}`).toBe(true)
  }
  // EXP-760: a sub-issue is one issues_create call with parentId.
  expect(playbook).toContain(`parentId`)
  // EXP-856: the workflow rule. A second copy of a live lane, resumed by
  // SendMessage into the same files, is the failure this sentence prevents.
  expect(playbook).toContain(`never messaged`)
  expect(playbook).toContain(`SendMessage`)
  // SLOP-3: follow-up runs base on the parent's branch through pr_open's base.
  expect(playbook).toContain(`base:`)
  expect(playbook).toContain(`follow-up`)
  expect(playbook).not.toContain(`stackOnIssueId`)
  expect(playbook).not.toContain(`\u2014`)
})

import { describe, expect, it, vi } from "vitest"

// S-3: `exponential_show` (a surface, ONE question or a plan) posts a card
// on the caller's own session, so it registers only inside a session; the
// three retired ops (plan_propose, sessions_ask, surface_show) are gone.
// Release train 2026-10-23: the host-version `cards` gate is gone (every
// desktop/CLI at the 0.14.75 floor feeds a session its own cards).

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
  inferBaseBranch: vi.fn(),
  ensureGithubStack: vi.fn(),
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

import {
  picturesOnlySurface,
  registerExponentialTools,
  SHOW_CARD_ROOT_ERROR,
  SHOW_TEXT_ONLY_ERROR,
  showSurfaceProblem,
} from "@/lib/emcp/tools"
import { buildSurfaceCard } from "@/lib/sessions/plan"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "@/lib/emcp/gates"
import { FULL_ACCESS } from "@/lib/emcp/scope"
import type { McpUser } from "@/lib/emcp/server"

const USER = {
  id: `user-1`,
  email: `u@example.com`,
  name: `User One`,
} as unknown as McpUser
const SESSION = `11111111-1111-4111-8111-111111111111`

function registered(gates: McpToolGates): Set<string> {
  const names = new Set<string>()
  const fakeServer = { registerTool: (name: string) => names.add(name) }
  registerExponentialTools(
    fakeServer as never,
    USER,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS,
    SESSION,
    gates
  )
  return names
}

const RETIRED = [`exponential_plan_propose`, `exponential_sessions_ask`, `exponential_surface_show`]

describe(`the show tool's registration (S-3)`, () => {
  it(`registers show inside a session, never the retired ops`, () => {
    const names = registered(ALL_MCP_TOOL_GATES)
    expect(names.has(`exponential_show`)).toBe(true)
    for (const tool of RETIRED) expect(names.has(tool), tool).toBe(false)
  })

  it(`does not register show outside a session`, () => {
    const names = registered({ ...ALL_MCP_TOOL_GATES, inSession: false })
    expect(names.has(`exponential_show`)).toBe(false)
  })
})

describe(`exponential_show with pictures only (F38)`, () => {
  it(`synthesizes a valid surface: a Card holding the title, else the labels`, () => {
    const card = buildSurfaceCard({
      surface: picturesOnlySurface(`Chats`, [{ label: `web chats` }]),
      title: `Chats`,
      pictures: [{ label: `web chats` }],
    })
    expect(card.surface?.root).toMatchObject({ component: `Card` })
    expect(JSON.stringify(card.surface)).toContain(`"Chats"`)
    expect(card.pictures?.map((picture) => picture.label)).toEqual([`web chats`])
    expect(JSON.stringify(picturesOnlySurface(undefined, [{ label: `web a` }, { label: `ios a` }]))).toContain(`web a, ios a`)
  })
})

// EXP-1445: the chat draws the card, so a Card root double-cards; text alone
// belongs in a plain message (the handler's refusal: tools.test.ts).
const node = (id: string, component: string, children?: unknown[]) => ({ id, component, ...(children ? { children } : {}) })

describe(`exponential_show refuses a Card root and text-only surfaces (EXP-1445)`, () => {
  it(`refuses a Card root, even with real components inside`, () => {
    const surface = { root: node(`c`, `Card`, [node(`b`, `Badge`), node(`x`, `Button`)]) }
    expect(showSurfaceProblem(surface, true)).toBe(SHOW_CARD_ROOT_ERROR)
  })

  it(`refuses a tree of only text and the containers arranging it`, () => {
    const surface = {
      root: node(`s`, `Stack`, [node(`h`, `Heading`), node(`sec`, `Section`, [node(`t`, `Text`), node(`sep`, `Separator`)]), node(`m`, `Markdown`), node(`l`, `Label`)]),
    }
    expect(showSurfaceProblem(surface, true)).toBe(SHOW_TEXT_ONLY_ERROR)
    expect(showSurfaceProblem({ root: node(`t`, `Text`) }, true)).toBe(SHOW_TEXT_ONLY_ERROR)
  })

  it(`passes once any real component appears, at any depth`, () => {
    for (const leaf of [`Badge`, `Row`, `StatTile`, `Table`, `Chart`, `Button`, `Progress`, `Image`]) {
      const surface = { root: node(`s`, `Stack`, [node(`h`, `Heading`), node(`b`, `Box`, [node(`x`, leaf)])]) }
      expect(showSurfaceProblem(surface, true), leaf).toBeNull()
    }
    expect(showSurfaceProblem({ root: { id: `l`, component: `List`, template: { component: `Row`, path: `/rows` } } }, true)).toBeNull()
  })

  it(`lets text ride beside ask, plan or pictures (the card is not text alone)`, () => {
    expect(showSurfaceProblem({ root: node(`t`, `Text`) }, false)).toBeNull()
    expect(showSurfaceProblem({ root: node(`c`, `Card`) }, false)).toBe(SHOW_CARD_ROOT_ERROR)
  })
})

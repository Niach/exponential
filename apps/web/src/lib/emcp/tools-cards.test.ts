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
  SHOW_INLINE_ONLY_ERROR,
  SHOW_TEXT_ONLY_ERROR,
  showSurfaceLinkProblem,
  showSurfaceProblem,
} from "@/lib/emcp/tools"
import { buildSurfaceCard } from "@/lib/sessions/plan"
import { decodeGenuiResult } from "@exp/mcp-apps/genui-result"
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

  it(`does not register show for a session that is not the caller's own chat`, () => {
    const names = registered({ ...ALL_MCP_TOOL_GATES, inSession: false })
    expect(names.has(`exponential_show`)).toBe(false)
  })
})

describe(`#VAPP-292: exponential_show for a client outside any session (a Claude connector)`, () => {
  type Def = { _meta?: Record<string, unknown> }
  type Handler = (args: Record<string, unknown>) => Promise<{ content: Array<{ text: string }>; isError?: boolean }>
  function outside() {
    const tools = new Map<string, { def: Def; handler: Handler }>()
    const fakeServer = { registerTool: (name: string, def: Def, handler: Handler) => tools.set(name, { def, handler }) }
    registerExponentialTools(fakeServer as never, USER, new Request(`https://x.test/api/mcp`), FULL_ACCESS, null, { ...ALL_MCP_TOOL_GATES, inSession: false })
    return tools.get(`exponential_show`)!
  }
  const surface = { root: { id: `s`, component: `Stack`, children: [{ id: `r`, component: `Row`, props: { title: `Tests` } }, { id: `b`, component: `Button`, props: { label: `Ship` }, on: { press: { event: { name: `reply`, context: { text: `Ship` } } } } }] } }

  it(`binds the genUI view and answers the card to draw inline`, async () => {
    const show = outside()
    expect(show.def._meta?.ui).toEqual({ resourceUri: `ui://exponential/genui` })
    const answer = await show.handler({ surface, title: `Release` })
    expect(answer.isError).toBeFalsy()
    const decoded = decodeGenuiResult(answer as never)
    expect(decoded.kind).toBe(`card`)
    expect(decoded.kind === `card` && decoded.card.title).toBe(`Release`)
  })

  it(`refuses a question, a plan and pictures (they need a session's chat)`, async () => {
    const show = outside()
    for (const args of [{ ask: { question: `Ship?`, options: [{ label: `Yes` }] } }, { plan: { text: `Do it` } }, { surface, pictures: [{ label: `web x` }] }]) {
      const answer = await show.handler(args)
      expect(answer.isError).toBe(true)
      expect(answer.content[0]!.text).toContain(SHOW_INLINE_ONLY_ERROR)
    }
  })

  it(`still refuses a Card root`, async () => {
    const answer = await outside().handler({ surface: { root: { id: `c`, component: `Card`, children: [{ id: `r`, component: `Row`, props: { title: `x` } }] } } })
    expect(answer.isError).toBe(true)
  })
})

describe(`exponential_show with pictures only (F38)`, () => {
  it(`synthesizes a valid surface: a Stack holding the title, else the labels`, () => {
    const card = buildSurfaceCard({
      surface: picturesOnlySurface(`Chats`, [{ label: `web chats` }]),
      title: `Chats`,
      pictures: [{ label: `web chats` }],
    })
    expect(card.surface?.root).toMatchObject({ component: `Stack` })
    // #EXP-1447: the chat draws the card; a Card root would double it.
    expect(showSurfaceProblem(picturesOnlySurface(`Chats`, [{ label: `web chats` }]), false)).toBeNull()
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

describe(`#EXP-1674: exponential_show refuses a link that opens nothing`, () => {
  const origin = `https://app.exponential.at`
  const surface = (root: Record<string, unknown>) => ({ root: { id: `s`, component: `Stack`, children: [root] } })

  it(`names the dead href, wherever it sits`, () => {
    expect(showSurfaceLinkProblem(surface({ id: `l`, component: `Link`, props: { label: `x`, href: `javascript:alert(1)` } }), origin, `acme`)).toContain(`javascript:alert(1)`)
    expect(showSurfaceLinkProblem(surface({ id: `r`, component: `Row`, props: { title: `x`, href: `https://…` } }), origin, `acme`)).toContain(`https://…`)
    expect(showSurfaceLinkProblem(surface({ id: `b`, component: `Button`, href: `#top` }), origin, `acme`)).toContain(`#top`)
    expect(showSurfaceLinkProblem(surface({ id: `r`, component: `Row`, slots: { trailing: { id: `b`, component: `Button`, props: { href: `~/../admin` } } } }), origin, `acme`)).toContain(`~/../admin`)
    expect(showSurfaceLinkProblem(surface({ id: `l`, component: `List`, template: { id: `t`, component: `Row`, props: { href: `//evil.example` } } }), origin, `acme`)).toContain(`//evil.example`)
  })

  it(`lets through what opens, and what it cannot read`, () => {
    for (const href of [`/t/acme/issues/EXP-1`, `~/settings/members`, `https://docs.rs/serde`, `${origin}/t/acme/inbox`, `mailto:a@example.com`, `{item.url}`, { path: `/url` }]) {
      expect(showSurfaceLinkProblem(surface({ id: `l`, component: `Link`, props: { href } }), origin, `acme`), JSON.stringify(href)).toBeNull()
    }
  })
})

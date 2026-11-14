// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// #FEED-218: `apps render` resolves a screen as one viewer sees it. The DB
// side (membership, the app row, records under the read policy) is mocked;
// what is real: the SDK host following the bindings, the records evaluator,
// the bound tree and its description.

const members: Record<string, string> = { owner: `owner`, ann: `member`, bob: `member` }
const access: Record<string, string> = { owner: `write`, ann: `write`, bob: `none` }
const rows: Record<string, { id: string; kind: string; data: Record<string, unknown>; createdBy: string; authorId: string; createdAt: string; updatedAt: string }[]> = {
  owner: [
    { id: `r1`, kind: `entry`, data: { label: `Mo`, hours: 8 }, createdBy: `owner`, authorId: `owner`, createdAt: `2026-11-10T08:00:00Z`, updatedAt: `2026-11-10T08:00:00Z` },
    { id: `r2`, kind: `entry`, data: { label: `Di`, hours: 7 }, createdBy: `owner`, authorId: `owner`, createdAt: `2026-11-11T08:00:00Z`, updatedAt: `2026-11-11T08:00:00Z` },
  ],
  ann: [],
}

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: vi.fn(async (userId: string) => {
    if (!members[userId]) throw new TRPCError({ code: `FORBIDDEN`, message: `Not a member` })
    return { role: members[userId] }
  }),
}))
vi.mock(`@/lib/team-user-ref`, () => ({
  resolveTeamUserRef: vi.fn(async (_team: string, ref: string) => (ref === `ann@example.com` ? `ann` : ref)),
}))

const pkg = {
  formatVersion: 5,
  id: `acme.hours`,
  name: `Hours`,
  version: `1.0.0`,
  catalogId: `https://ui.exponential.at/catalogs/exponential-app/v1`,
  sources: [`records:*`],
  records: { entry: { schema: { type: `object`, properties: { label: { type: `string` }, hours: { type: `number` } }, required: [`label`, `hours`] } } },
  screens: [
    { template: `main`, title: `Hours` },
    { template: `other`, title: `Other` },
  ],
  templates: {
    main: {
      bindings: [{ path: `/entries`, source: `records:entry` }],
      components: [
        { id: `root`, component: `Stack`, children: [`total`, `list`] },
        { id: `total`, component: `Heading`, text: { call: `expr`, args: { expr: `string(entries.count) + ' entries'`, vars: { entries: { path: `/entries` } }, default: `?` } } },
        { id: `list`, component: `List`, children: { componentId: `row`, path: `/entries/rows` } },
        { id: `row`, component: `Row`, title: { path: `label` } },
      ],
    },
    other: { components: [{ id: `root`, component: `Text`, text: `Other` }] },
  },
}

vi.mock(`@/lib/apps/records-service`, () => ({
  loadApp: vi.fn(async () => ({ id: `app1`, teamId: `team1`, slug: `hours`, name: `Hours`, headVersion: 2, publishedVersion: 1, package: pkg, draftPackage: null })),
  appAccessLevel: vi.fn(async (_db: unknown, _app: string, userId: string) => access[userId] ?? `write`),
  readAppRecordRows: vi.fn(async (_app: unknown, _kind: string, _contains: unknown, userId: string) => ({ rows: rows[userId] ?? [], truncated: false })),
  readRefLookups: vi.fn(async () => ({ refs: undefined, targets: [], issues: {} })),
  teamMemberNames: vi.fn(async () => ({ owner: `Owner`, ann: `Ann` })),
}))

const { renderAppScreen } = await import(`@/lib/apps/render`)

/** The teams read (no zone). */
const executor = { select: () => ({ from: () => ({ where: () => ({ limit: async () => [{ timeZone: null }] }) }) }) } as never

describe(`renderAppScreen`, () => {
  beforeEach(() => vi.clearAllMocks())

  it(`renders the first screen with its records bound, as the caller`, async () => {
    const out = await renderAppScreen({ userId: `owner`, teamId: `team1`, slug: `hours` }, executor)
    expect(out.screen).toBe(`main`)
    expect(out.viewer).toEqual({ id: `owner`, role: `owner`, access: `write` })
    expect(out.nav).toBe(`sidebar: [Hours] · Other`)
    expect(out.outline).toContain(`Heading#total text="2 entries"`)
    expect(out.outline).toContain(`× 2 row`)
    expect(out.outline).toContain(`Row#row title="Mo"`)
    expect(out.issues).toEqual([])
  })

  it(`asUser renders their records (an owner only) and reports the empty list`, async () => {
    const out = await renderAppScreen({ userId: `owner`, teamId: `team1`, slug: `hours`, asUser: `ann@example.com`, formFactor: `phone` }, executor)
    expect(out.viewer.id).toBe(`ann`)
    expect(out.nav).toBe(`tab bar: [Hours] · Other`)
    expect(out.outline).toContain(`text="0 entries"`)
    expect(out.issues).toEqual([`list: the list at /entries/rows is empty`])
  })

  it(`a member may not render as another one`, async () => {
    await expect(renderAppScreen({ userId: `ann`, teamId: `team1`, slug: `hours`, asUser: `owner` }, executor)).rejects.toThrow(/Only a team owner/)
  })

  it(`a viewer at access none gets NOT_FOUND`, async () => {
    await expect(renderAppScreen({ userId: `bob`, teamId: `team1`, slug: `hours` }, executor)).rejects.toThrow(/App not found/)
    await expect(renderAppScreen({ userId: `owner`, teamId: `team1`, slug: `hours`, asUser: `bob` }, executor)).rejects.toThrow(/no access/)
  })

  it(`names the screens when one does not exist`, async () => {
    await expect(renderAppScreen({ userId: `owner`, teamId: `team1`, slug: `hours`, screen: `nope` }, executor)).rejects.toThrow(/screens are main, other/)
  })

  it(`a named screen renders that template`, async () => {
    const out = await renderAppScreen({ userId: `owner`, teamId: `team1`, slug: `hours`, screen: `other` }, executor)
    expect(out.outline).toBe(`Text#root text="Other"`)
  })
})

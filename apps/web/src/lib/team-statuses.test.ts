import { describe, expect, it } from "vitest"
import { BUILTIN_STATUS_DEFAULTS } from "@/lib/domain"
import {
  buildStatusOptions,
  creatableStatusOptions,
  defaultStatusOptions,
  fallbackStatusOptions,
  isFallbackStatusOption,
  resolveIssueStatus,
  statusFilterToken,
  statusOptionMatchesToken,
  statusUpdatePayload,
  type StatusRowInput,
} from "@/lib/team-statuses"

function row(overrides: Partial<StatusRowInput> & { id: string }): StatusRowInput {
  return {
    name: `Status`,
    color: `#A1A1AA`,
    category: `backlog`,
    builtinKey: null,
    sortOrder: 1,
    createdAt: new Date(`2026-01-01T00:00:00.000Z`),
    ...overrides,
  }
}

describe(`buildStatusOptions`, () => {
  it(`orders by category display order, then sortOrder, createdAt, id`, () => {
    const options = buildStatusOptions([
      row({ id: `dup`, category: `duplicate` }),
      row({ id: `done`, category: `completed` }),
      row({ id: `backlog`, category: `backlog` }),
      row({ id: `todo`, category: `unstarted` }),
      row({ id: `cancelled`, category: `cancelled` }),
      row({ id: `progress`, category: `started` }),
    ])

    expect(options.map((option) => option.id)).toEqual([
      `progress`,
      `todo`,
      `backlog`,
      `done`,
      `cancelled`,
      `dup`,
    ])
  })

  it(`breaks ties by sortOrder, then createdAt, then id`, () => {
    const options = buildStatusOptions([
      row({
        id: `b`,
        category: `started`,
        sortOrder: 2,
        createdAt: new Date(`2026-01-01T00:00:00.000Z`),
      }),
      row({
        id: `c`,
        category: `started`,
        sortOrder: 1,
        createdAt: new Date(`2026-02-01T00:00:00.000Z`),
      }),
      row({
        id: `a`,
        category: `started`,
        sortOrder: 1,
        createdAt: new Date(`2026-01-01T00:00:00.000Z`),
      }),
      row({
        id: `a0`,
        category: `started`,
        sortOrder: 1,
        createdAt: new Date(`2026-01-01T00:00:00.000Z`),
      }),
    ])

    expect(options.map((option) => option.id)).toEqual([`a`, `a0`, `c`, `b`])
  })

  it(`derives started clocks from position among started rows`, () => {
    const one = buildStatusOptions([row({ id: `s1`, category: `started` })])
    expect(one.map((option) => option.icon)).toEqual([`progress-2-4`])

    const two = buildStatusOptions([
      row({ id: `s1`, category: `started`, sortOrder: 1 }),
      row({ id: `s2`, category: `started`, sortOrder: 2 }),
    ])
    expect(two.map((option) => option.icon)).toEqual([
      `progress-2-4`,
      `progress-3-4`,
    ])

    const three = buildStatusOptions([
      row({ id: `s1`, category: `started`, sortOrder: 1 }),
      row({ id: `s2`, category: `started`, sortOrder: 2 }),
      row({ id: `s3`, category: `started`, sortOrder: 3 }),
    ])
    expect(three.map((option) => option.icon)).toEqual([
      `progress-1-4`,
      `progress-2-4`,
      `progress-3-4`,
    ])

    const four = buildStatusOptions([
      row({ id: `s1`, category: `started`, sortOrder: 1 }),
      row({ id: `s2`, category: `started`, sortOrder: 2 }),
      row({ id: `s3`, category: `started`, sortOrder: 3 }),
      row({ id: `s4`, category: `started`, sortOrder: 4 }),
    ])
    expect(four.map((option) => option.icon)).toEqual([
      `progress-1-5`,
      `progress-2-5`,
      `progress-3-5`,
      `progress-4-5`,
    ])
  })

  it(`gives each non-started category its fixed glyph`, () => {
    const options = buildStatusOptions([
      row({ id: `b`, category: `backlog` }),
      row({ id: `u`, category: `unstarted` }),
      row({ id: `c`, category: `completed` }),
      row({ id: `x`, category: `cancelled` }),
      row({ id: `d`, category: `duplicate` }),
    ])

    expect(
      Object.fromEntries(options.map((o) => [o.category, o.icon]))
    ).toEqual({
      backlog: `circle-dashed`,
      unstarted: `circle`,
      completed: `circle-check`,
      cancelled: `circle-x`,
      duplicate: `copy`,
    })
  })
})

describe(`fallbackStatusOptions`, () => {
  it(`mirrors the contract defaults with synthetic builtin ids`, () => {
    const options = fallbackStatusOptions()
    expect(options).toHaveLength(BUILTIN_STATUS_DEFAULTS.length)

    for (const entry of BUILTIN_STATUS_DEFAULTS) {
      const option = options.find((o) => o.builtinKey === entry.key)!
      expect(option).toBeDefined()
      expect(option.id).toBe(`builtin:${entry.key}`)
      expect(option.name).toBe(entry.name)
      expect(option.colorHex).toBe(entry.color)
      expect(option.category).toBe(entry.category)
      expect(option.sortOrder).toBe(entry.sortOrder)
      expect(isFallbackStatusOption(option)).toBe(true)
    }
  })

  it(`renders the default team in the legacy display order`, () => {
    expect(defaultStatusOptions().map((option) => option.builtinKey)).toEqual([
      `in_progress`,
      `in_review`,
      `todo`,
      `backlog`,
      `done`,
      `cancelled`,
      `duplicate`,
    ])
  })

  it(`keeps the builtin In Progress / In Review clock pair`, () => {
    const byKey = new Map(
      defaultStatusOptions().map((option) => [option.builtinKey, option.icon])
    )
    expect(byKey.get(`in_progress`)).toBe(`progress-2-4`)
    expect(byKey.get(`in_review`)).toBe(`progress-3-4`)
  })
})

describe(`resolveIssueStatus`, () => {
  const options = buildStatusOptions([
    row({
      id: `11111111-1111-1111-1111-111111111111`,
      name: `Backlog`,
      category: `backlog`,
      builtinKey: `backlog`,
    }),
    row({
      id: `22222222-2222-2222-2222-222222222222`,
      name: `Todo`,
      category: `unstarted`,
      builtinKey: `todo`,
    }),
    row({
      id: `33333333-3333-3333-3333-333333333333`,
      name: `Designing`,
      category: `started`,
    }),
  ])
  const byId = new Map(options.map((option) => [option.id, option]))

  it(`prefers the statusId row`, () => {
    expect(
      resolveIssueStatus(
        {
          status: `backlog`,
          statusId: `33333333-3333-3333-3333-333333333333`,
        },
        options,
        byId
      ).name
    ).toBe(`Designing`)
  })

  it(`falls back to the anchor-enum row`, () => {
    expect(
      resolveIssueStatus({ status: `todo`, statusId: null }, options, byId).name
    ).toBe(`Todo`)
  })

  it(`falls back to a constructed default when no team row anchors it`, () => {
    const resolved = resolveIssueStatus(
      { status: `done`, statusId: null },
      options,
      byId
    )
    expect(resolved.id).toBe(`builtin:done`)
    expect(resolved.name).toBe(`Done`)
  })

  it(`falls back to constructed Backlog for unknown/forward-compat values`, () => {
    expect(
      resolveIssueStatus({ status: `triaged`, statusId: null }, options, byId)
        .id
    ).toBe(`builtin:backlog`)
    expect(
      resolveIssueStatus({ status: `triaged`, statusId: `nope` }, options, byId)
        .id
    ).toBe(`builtin:backlog`)
  })

  it(`works without a prebuilt byId map`, () => {
    expect(
      resolveIssueStatus(
        {
          status: `backlog`,
          statusId: `22222222-2222-2222-2222-222222222222`,
        },
        options
      ).name
    ).toBe(`Todo`)
  })
})

describe(`status write payloads`, () => {
  it(`writes statusId for real rows and the anchor enum for fallbacks`, () => {
    const [custom] = buildStatusOptions([
      row({ id: `44444444-4444-4444-4444-444444444444`, category: `started` }),
    ])
    expect(statusUpdatePayload(custom)).toEqual({
      statusId: `44444444-4444-4444-4444-444444444444`,
    })

    const fallbackDone = defaultStatusOptions().find(
      (option) => option.builtinKey === `done`
    )!
    expect(statusUpdatePayload(fallbackDone)).toEqual({ status: `done` })
  })
})

describe(`creatableStatusOptions / statusOptionMatchesToken`, () => {
  it(`drops the duplicate category from create pickers`, () => {
    expect(
      creatableStatusOptions(defaultStatusOptions()).map((o) => o.builtinKey)
    ).not.toContain(`duplicate`)
  })

  // `?status=` accepts uuids and anchor enums only — a synthetic
  // `builtin:<key>` id would be stripped by the route's validateSearch.
  it(`emits URL-safe filter tokens`, () => {
    const [custom] = buildStatusOptions([
      row({ id: `66666666-6666-6666-6666-666666666666`, category: `started` }),
    ])
    expect(statusFilterToken(custom)).toBe(
      `66666666-6666-6666-6666-666666666666`
    )
    for (const option of defaultStatusOptions()) {
      expect(statusFilterToken(option)).toBe(option.builtinKey)
      expect(statusFilterToken(option).startsWith(`builtin:`)).toBe(false)
    }
  })

  it(`matches row-id and legacy enum tokens`, () => {
    const [option] = buildStatusOptions([
      row({
        id: `55555555-5555-5555-5555-555555555555`,
        category: `completed`,
        builtinKey: `done`,
      }),
    ])
    expect(
      statusOptionMatchesToken(option, `55555555-5555-5555-5555-555555555555`)
    ).toBe(true)
    expect(statusOptionMatchesToken(option, `done`)).toBe(true)
    expect(statusOptionMatchesToken(option, `todo`)).toBe(false)
  })
})

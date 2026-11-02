import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

const { headSession, pushAuthor, pushTime, PUSH_GRACE_MS } = await import(`@/lib/sessions/push-attribution`)

// #EXP-1540: who a push is credited to, and who a session-less PR resolves to.
const now = new Date(`2026-11-01T12:00:00Z`)
const c = (id: string, extra: Partial<Parameters<typeof pushAuthor>[0][number]> = {}) => ({
  id,
  status: `running`,
  busy: false,
  background: false,
  endedAt: null,
  ...extra,
})

describe(`pushAuthor`, () => {
  it(`one live session on the branch = it, busy or not`, () => {
    expect(pushAuthor([c(`a`)], now)).toBe(`a`)
    expect(pushAuthor([c(`a`, { status: `idle` })], now)).toBe(`a`)
    expect(pushAuthor([], now)).toBeNull()
  })

  it(`several: the only one mid-turn (busy, background, or just ended)`, () => {
    expect(pushAuthor([c(`a`), c(`b`, { busy: true })], now)).toBe(`b`)
    expect(pushAuthor([c(`a`), c(`b`, { background: true })], now)).toBe(`b`)
    const justEnded = c(`b`, { status: `ended`, endedAt: new Date(now.getTime() - 30_000) })
    expect(pushAuthor([c(`a`), justEnded], now)).toBe(`b`)
  })

  it(`still ambiguous = nobody, never the newest`, () => {
    expect(pushAuthor([c(`a`), c(`b`)], now)).toBeNull()
    expect(pushAuthor([c(`a`, { busy: true }), c(`b`, { busy: true })], now)).toBeNull()
  })

  it(`an ended row the grace window still admits counts; its edge is inclusive`, () => {
    // Rows ended longer ago never reach pushAuthor (attributePush's SQL
    // filters them; the real-DB case covers that edge).
    const edge = c(`b`, { status: `ended`, endedAt: new Date(now.getTime() - PUSH_GRACE_MS) })
    expect(pushAuthor([c(`a`), edge], now)).toBe(`b`)
  })
})

describe(`headSession`, () => {
  it(`the session credited with the head's latest push wins over two lineages sitting on it`, () => {
    const tree = [
      { id: `a`, parentId: `r`, status: `running`, branch: `exp/x` },
      { id: `b`, parentId: `r`, status: `running`, branch: `exp/x` },
    ]
    expect(headSession(tree, `exp/x`, `b`)).toBe(`b`)
  })

  it(`a pusher that left the head or is outside the tree never counts; then the one lineage on it, else nobody`, () => {
    const one = [
      { id: `a`, parentId: null, status: `ended`, branch: `exp/y` },
      { id: `b`, parentId: null, continuedFromId: `a`, status: `running`, branch: `exp/x` },
    ]
    expect(headSession(one, `exp/x`, `a`)).toBe(`b`)
    expect(headSession(one, `exp/x`, `elsewhere`)).toBe(`b`)
    const two = [...one, { id: `c`, parentId: null, status: `running`, branch: `exp/x` }]
    expect(headSession(two, `exp/x`, `a`)).toBeNull()
    expect(headSession(two, `exp/x`, `c`)).toBe(`c`)
  })

  it(`the latest push was nobody's: no pusher, the fallback decides`, () => {
    const tree = [
      { id: `a`, parentId: `r`, status: `running`, branch: `exp/x` },
      { id: `b`, parentId: `r`, status: `running`, branch: `exp/x` },
    ]
    expect(headSession(tree, `exp/x`, null)).toBeNull()
  })
})

// #EXP-1540: the branch row only moves forward, so a late older push never
// credits anyone (the rule `attributePush` writes in SQL).
describe(`attributePush ordering`, () => {
  const rows = new Map<string, { pushedAt: Date; sessionId: string | null }>()
  const candidates: { id: string; repositoryId: string; status: string; busy: boolean; background: boolean; endedAt: null }[] = []
  beforeEach(() => {
    rows.clear()
    candidates.length = 0
  })
  const chain = (result: unknown) => {
    const it: Record<string, unknown> = {}
    for (const key of [`from`, `where`]) it[key] = () => it
    it.then = (resolve: (value: unknown) => unknown) => resolve(result)
    return it
  }
  beforeAll(async () => {
    const { db } = (await import(`@/db/connection`)) as unknown as { db: Record<string, unknown> }
    let selects = 0
    db.select = () => chain(selects++ % 2 === 0 ? [{ id: `repo` }] : candidates)
    db.insert = () => ({
      values: (values: { branch: string; pushedAt: Date; sessionId: string | null }[]) => ({
        onConflictDoUpdate: () => ({
          returning: async () =>
            values.flatMap((value) => {
              const prior = rows.get(value.branch)
              if (prior && prior.pushedAt >= value.pushedAt) return []
              rows.set(value.branch, { pushedAt: value.pushedAt, sessionId: value.sessionId })
              return [{ sessionId: value.sessionId }]
            }),
        }),
      }),
    })
  })

  it(`a newer uncredited push, then an older credited one delivered late: nobody is credited`, async () => {
    const { attributePush } = await import(`@/lib/sessions/push-attribution`)
    const push = (at: Date) => attributePush({ fullName: `org/repo`, branch: `exp/x`, at, now })
    candidates.push(
      { id: `a`, repositoryId: `repo`, status: `running`, busy: false, background: false, endedAt: null },
      { id: `b`, repositoryId: `repo`, status: `running`, busy: false, background: false, endedAt: null }
    )
    expect(await push(new Date(now.getTime() - 30_000))).toBeNull()
    candidates[0]!.busy = true
    expect(await push(new Date(now.getTime() - 60_000))).toBeNull()
    expect(rows.get(`exp/x`)?.sessionId).toBeNull()
    // A genuinely newer credited push still credits.
    expect(await push(new Date(now.getTime() - 10_000))).toBe(`a`)
  })
})

describe(`pushTime`, () => {
  it(`the head commit's time, else the repository's pushed_at, else none; never in the future`, () => {
    expect(pushTime({ head_commit: { timestamp: `2026-10-01T10:00:00+02:00` } }, now)).toEqual(
      new Date(`2026-10-01T08:00:00Z`)
    )
    expect(pushTime({ head_commit: null, repository: { pushed_at: 1_790_000_000 } }, now)).toEqual(
      new Date(1_790_000_000_000)
    )
    expect(pushTime({ head_commit: { timestamp: `garbage` } }, now)).toBeUndefined()
    expect(pushTime({}, now)).toBeUndefined()
    expect(pushTime({ head_commit: { timestamp: `2027-01-01T00:00:00Z` } }, now)).toEqual(now)
  })
})

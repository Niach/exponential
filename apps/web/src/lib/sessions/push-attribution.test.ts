import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

const { headSession, pushAuthor, PUSH_GRACE_MS } = await import(`@/lib/sessions/push-attribution`)

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
    const longEnded = c(`b`, { status: `ended`, endedAt: new Date(now.getTime() - PUSH_GRACE_MS - 1) })
    expect(pushAuthor([c(`a`), longEnded], now)).toBe(`a`)
  })
})

describe(`headSession`, () => {
  const at = (minutes: number) => new Date(now.getTime() + minutes * 60_000)

  it(`the session that pushed the head wins over two lineages sitting on it`, () => {
    const tree = [
      { id: `a`, parentId: `r`, status: `running`, branch: `exp/x`, pushedBranch: `exp/x`, pushedAt: at(1) },
      { id: `b`, parentId: `r`, status: `running`, branch: `exp/x`, pushedBranch: `exp/x`, pushedAt: at(2) },
      { id: `c`, parentId: `r`, status: `running`, branch: `exp/x` },
    ]
    expect(headSession(tree, `exp/x`)).toBe(`b`)
  })

  it(`a push of another branch never counts; then the one lineage on the head, else nobody`, () => {
    const one = [
      { id: `a`, parentId: null, status: `ended`, branch: `exp/x`, pushedBranch: `exp/y`, pushedAt: at(1) },
      { id: `b`, parentId: null, continuedFromId: `a`, status: `running`, branch: `exp/x` },
    ]
    expect(headSession(one, `exp/x`)).toBe(`b`)
    expect(headSession(one, `exp/y`)).toBe(`a`)
    const two = [...one, { id: `c`, parentId: null, status: `running`, branch: `exp/x` }]
    expect(headSession(two, `exp/x`)).toBeNull()
  })
})

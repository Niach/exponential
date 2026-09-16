import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-897: the session CHAIN (one recursive walk up `parent_session_id`) and
// the descendant-question format an escalation past the immediate parent uses.

const h = vi.hoisted(() => ({
  executeRows: [] as Record<string, unknown>[],
  executeCalls: [] as string[],
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: () => null,
  relayPostInput: vi.fn(),
}))

import {
  formatDescendantQuestion,
  loadSessionChain,
  loadSessionDepths,
  loadSubtreeSessionIds,
} from "@/lib/steer-child-messages"

const execute = vi.fn(async (query: unknown) => {
  h.executeCalls.push(JSON.stringify(query))
  return { rows: h.executeRows }
})
const db = { execute } as never

function chainRow(
  id: string,
  depth: number,
  over: Record<string, unknown> = {}
) {
  return {
    id,
    user_id: `owner`,
    host_user_id: null,
    team_id: `ws-1`,
    status: `running`,
    started_reason: `agent`,
    parent_session_id: null,
    action_name: null,
    issue_identifier: null,
    depth,
    ...over,
  }
}

beforeEach(() => {
  h.executeRows = []
  h.executeCalls.length = 0
  vi.clearAllMocks()
})

describe(`loadSessionChain`, () => {
  it(`reports depth, the root and the highest LIVE ancestor`, async () => {
    h.executeRows = [
      chainRow(`self`, 0, { parent_session_id: `parent` }),
      chainRow(`parent`, 1, { parent_session_id: `root`, status: `ended` }),
      chainRow(`root`, 2, { status: `running`, started_reason: null }),
    ]
    const chain = await loadSessionChain(db, `self`)
    expect(chain).toMatchObject({
      depth: 2,
      rootSessionId: `root`,
      topLiveAncestorId: `root`,
    })
    expect(chain!.ancestors.map((row) => row.id)).toEqual([`parent`, `root`])
  })

  it(`has no live ancestor when every one of them ended`, async () => {
    h.executeRows = [
      chainRow(`self`, 0, { parent_session_id: `parent` }),
      chainRow(`parent`, 1, { status: `ended` }),
    ]
    const chain = await loadSessionChain(db, `self`)
    expect(chain?.topLiveAncestorId).toBeNull()
    expect(chain?.rootSessionId).toBe(`parent`)
  })

  it(`treats a root run as depth 0 whose root is itself`, async () => {
    h.executeRows = [chainRow(`self`, 0)]
    const chain = await loadSessionChain(db, `self`)
    expect(chain).toMatchObject({
      depth: 0,
      rootSessionId: `self`,
      topLiveAncestorId: null,
    })
  })

  it(`answers null for an unknown session`, async () => {
    h.executeRows = []
    await expect(loadSessionChain(db, `gone`)).resolves.toBeNull()
  })

  it(`passes the depth cap into the recursive walk`, async () => {
    h.executeRows = [chainRow(`self`, 0)]
    await loadSessionChain(db, `self`, 5)
    expect(h.executeCalls[0]).toContain(`5`)
  })
})

describe(`loadSessionDepths / loadSubtreeSessionIds`, () => {
  it(`maps each requested id onto its depth`, async () => {
    h.executeRows = [
      { origin_id: `a`, depth: 0 },
      { origin_id: `b`, depth: 2 },
    ]
    const depths = await loadSessionDepths(db, [`a`, `b`])
    expect(depths.get(`a`)).toBe(0)
    expect(depths.get(`b`)).toBe(2)
  })

  it(`never queries for an empty page`, async () => {
    const depths = await loadSessionDepths(db, [])
    expect(depths.size).toBe(0)
    expect(execute).not.toHaveBeenCalled()
  })

  it(`returns the subtree ids, root included`, async () => {
    h.executeRows = [{ id: `root` }, { id: `child` }]
    await expect(loadSubtreeSessionIds(db, `root`)).resolves.toEqual([
      `root`,
      `child`,
    ])
  })
})

describe(`formatDescendantQuestion`, () => {
  it(`names how far down the asker sits and its full session id`, () => {
    expect(
      formatDescendantQuestion(
        { id: `3f2a9c1b-0000-4000-8000-000000000000`, issueIdentifier: `EXP-12`, actionName: null },
        `Should we keep the old column?`,
        3
      )
    ).toBe(
      `[Exponential child run EXP-12 3f2a9c1b (3 levels down) asks — reply with exponential_sessions_message sessionId=3f2a9c1b-0000-4000-8000-000000000000] Should we keep the old column?`
    )
  })

  it(`says "1 level down" in the singular and collapses newlines`, () => {
    const text = formatDescendantQuestion(
      { id: `aaaaaaaa-0000-4000-8000-000000000000`, issueIdentifier: null, actionName: `Nightly` },
      `line one\nline two`,
      1
    )
    expect(text).toContain(`(1 level down)`)
    expect(text).toContain(`line one line two`)
    expect(text).not.toContain(`\n`)
  })
})

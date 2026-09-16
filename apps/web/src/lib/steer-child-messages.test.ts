import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-897: the session CHAIN (one recursive walk up `parent_session_id`) and
// the descendant-question format an escalation past the immediate parent uses.

const h = vi.hoisted(() => ({
  executeRows: [] as Record<string, unknown>[],
  executeCalls: [] as string[],
  relayConfig: null as { url: string; secret: string } | null,
  // EXP-906: the child row loadChildParentContext's select serves.
  selectRows: [] as Record<string, unknown>[],
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: () => h.relayConfig,
  relayPostInput: vi.fn(async () => ({ delivered: true })),
}))

import { relayPostInput } from "@/lib/steer"
import {
  formatDescendantQuestion,
  loadSessionChain,
  loadSessionDepths,
  loadSubtreeSessionIds,
  notifyParentOfChildBlocked,
  notifyParentOfChildEnd,
  resolveLiveParentSessionId,
} from "@/lib/steer-child-messages"

const execute = vi.fn(async (query: unknown) => {
  h.executeCalls.push(JSON.stringify(query))
  return { rows: h.executeRows }
})
// The one select chain loadChildParentContext builds (self-join on the
// parent), resolving to `h.selectRows`.
const selectChain = {
  from: () => selectChain,
  leftJoin: () => selectChain,
  where: () => selectChain,
  limit: async () => h.selectRows,
}
const db = { execute, select: () => selectChain } as never

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
  h.relayConfig = null
  h.selectRows = []
  vi.clearAllMocks()
})

// EXP-906: the parent of the 2026-09-15 stack switched account mid-run, which
// ended its row and relaunched it under a new id (`resumed_from_id` chain) —
// every child kept pointing at the ended predecessor and went silent.
describe(`resolveLiveParentSessionId`, () => {
  it(`answers the parent itself while it is live, without a query`, async () => {
    await expect(
      resolveLiveParentSessionId(db, {
        parentSessionId: `parent`,
        parentStatus: `in_review`,
      })
    ).resolves.toBe(`parent`)
    expect(execute).not.toHaveBeenCalled()
  })

  it(`follows an ended parent's resume succession to its live successor`, async () => {
    h.executeRows = [{ id: `parent-v3` }]
    await expect(
      resolveLiveParentSessionId(db, {
        parentSessionId: `parent`,
        parentStatus: `ended`,
      })
    ).resolves.toBe(`parent-v3`)
    expect(h.executeCalls[0]).toContain(`resumed_from_id`)
    expect(h.executeCalls[0]).toContain(`parent`)
  })

  it(`answers null with no parent or when the whole succession ended`, async () => {
    await expect(
      resolveLiveParentSessionId(db, {
        parentSessionId: null,
        parentStatus: null,
      })
    ).resolves.toBeNull()
    h.executeRows = []
    await expect(
      resolveLiveParentSessionId(db, {
        parentSessionId: `parent`,
        parentStatus: `ended`,
      })
    ).resolves.toBeNull()
  })
})

describe(`notifyParentOfChildEnd / notifyParentOfChildBlocked (EXP-906)`, () => {
  const child = {
    id: `aaaaaaaa-0000-4000-8000-000000000000`,
    userId: `owner`,
    hostUserId: null,
    startedReason: `agent`,
    parentSessionId: `parent`,
    actionName: null,
    issueIdentifier: `EXP-12`,
    parentStatus: `ended`,
  }

  it(`delivers the child's finish to the parent's live successor`, async () => {
    h.relayConfig = { url: `https://relay.test`, secret: `s` }
    h.selectRows = [child]
    h.executeRows = [{ id: `parent-v2` }]

    const result = await notifyParentOfChildEnd(db, child.id, {
      summary: `done`,
      endedBy: `agent`,
    })

    expect(result).toEqual({ delivered: true })
    expect(relayPostInput).toHaveBeenCalledWith(
      h.relayConfig,
      `parent-v2`,
      `[Exponential child run EXP-12 aaaaaaaa finished] done`
    )
  })

  it(`delivers the usage wall to the live successor too`, async () => {
    h.relayConfig = { url: `https://relay.test`, secret: `s` }
    h.selectRows = [child]
    h.executeRows = [{ id: `parent-v2` }]

    await notifyParentOfChildBlocked(db, child.id, {
      resetsAt: `2026-09-16T00:10:00.000Z`,
      window: `session`,
    })

    expect(relayPostInput).toHaveBeenCalledWith(
      h.relayConfig,
      `parent-v2`,
      expect.stringContaining(`is rate limited (session window) until`)
    )
  })

  it(`stays silent when every run in the succession ended`, async () => {
    h.relayConfig = { url: `https://relay.test`, secret: `s` }
    h.selectRows = [child]
    h.executeRows = []

    await expect(
      notifyParentOfChildEnd(db, child.id, { summary: null, endedBy: `client` })
    ).resolves.toEqual({ delivered: false })
    expect(relayPostInput).not.toHaveBeenCalled()
  })
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

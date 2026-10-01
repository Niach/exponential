import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-700/906: the child → parent notices and the resume-succession walks.

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
  notifyParentOfChildBlocked,
  notifyParentOfChildEnd,
  notifyParentOfChildResumed,
  findLiveResumeId,
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

// FEED-68: a walled child was ended by its device and auto-resumed two
// seconds later; the parent, told only "ended without a report", resumed it a
// second time into the same worktree.
describe(`a child that resumes itself (FEED-68)`, () => {
  const child = {
    id: `aaaaaaaa-0000-4000-8000-000000000000`,
    userId: `owner`,
    hostUserId: null,
    startedReason: `agent`,
    parentSessionId: `parent`,
    actionName: null,
    issueIdentifier: `EXP-12`,
    parentStatus: `running`,
  }

  it(`says the run resumes itself instead of "ended without a report"`, async () => {
    h.relayConfig = { url: `https://relay.test`, secret: `s` }
    h.selectRows = [child]

    await notifyParentOfChildEnd(db, child.id, {
      summary: null,
      endedBy: `client`,
      resuming: true,
    })

    expect(relayPostInput).toHaveBeenCalledWith(
      h.relayConfig,
      `parent`,
      `[Exponential child run EXP-12 aaaaaaaa is switching accounts and resumes itself under a new id — do NOT resume or restart it]`
    )
  })

  it(`names the successor's full id once the resume landed`, async () => {
    h.relayConfig = { url: `https://relay.test`, secret: `s` }
    const successor = `bbbbbbbb-0000-4000-8000-000000000000`
    // The successor's row: it inherited the parent link and the reason.
    h.selectRows = [{ ...child, id: successor }]

    const result = await notifyParentOfChildResumed(db, child.id, successor)

    expect(result).toEqual({ delivered: true })
    expect(relayPostInput).toHaveBeenCalledWith(
      h.relayConfig,
      `parent`,
      `[Exponential child run EXP-12 aaaaaaaa resumed as ${successor} — it is live under that id: track and message it there, do NOT resume aaaaaaaa]`
    )
  })

  it(`says nothing for a resume of a person-started run`, async () => {
    h.relayConfig = { url: `https://relay.test`, secret: `s` }
    h.selectRows = [{ ...child, startedReason: null, parentSessionId: null }]

    await expect(
      notifyParentOfChildResumed(db, child.id, `successor`)
    ).resolves.toEqual({ delivered: false })
    expect(relayPostInput).not.toHaveBeenCalled()
  })

  it(`finds the newest live resume of an ended run`, async () => {
    h.executeRows = [{ id: `resume-2` }]
    await expect(findLiveResumeId(db, child.id)).resolves.toBe(`resume-2`)
    // Walks the succession forward from the run, the run itself excluded.
    expect(h.executeCalls[0]).toContain(`resumed_from_id`)
    h.executeRows = []
    await expect(findLiveResumeId(db, child.id)).resolves.toBeNull()
  })
})

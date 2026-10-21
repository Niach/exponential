import { renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  send: vi.fn(),
  toastActionError: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { sessions: { send: { mutate: mocks.send } } },
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  toastActionError: mocks.toastActionError,
}))

import { btwQuestion, useSteerCommand } from "@/hooks/use-steer-command"

// EXP-1323 (#EXP-1330): `/btw <question>` is the only steer command the web
// dispatches itself (an aside child); everything else stays message text.
describe(`btwQuestion`, () => {
  it(`reads the question of a claude session's /btw`, () => {
    expect(btwQuestion(`/btw what does useCart return?`, `claude`)).toBe(
      `what does useCart return?`
    )
    expect(btwQuestion(`  /BTW  why?  `, null)).toBe(`why?`)
  })

  it(`leaves other commands, prose, a bare /btw and other agents alone`, () => {
    expect(btwQuestion(`/compact`, `claude`)).toBeNull()
    expect(btwQuestion(`by the way /btw`, `claude`)).toBeNull()
    expect(btwQuestion(`/btw`, `claude`)).toBeNull()
    expect(btwQuestion(`/btw why?`, `codex`)).toBeNull()
  })
})

describe(`useSteerCommand`, () => {
  beforeEach(() => {
    mocks.send.mockReset()
    mocks.toastActionError.mockReset()
  })

  const dispatch = (agent: string | null) =>
    renderHook(() => useSteerCommand({ teamId: `t1`, sessionId: `s1`, agent })).result.current

  it(`sends /btw as a child of the session (parentId) and reports sent`, async () => {
    mocks.send.mockResolvedValue({ sessionId: `s2` })
    await expect(dispatch(`claude`)(`/btw why?`)).resolves.toBe(`sent`)
    expect(mocks.send).toHaveBeenCalledTimes(1)
    expect(mocks.send.mock.calls[0]![0]).toEqual({
      teamId: `t1`,
      parentId: `s1`,
      text: `why?`,
    })
    // The hook raises its own toast: the generic mutation toast is skipped.
    expect(mocks.send.mock.calls[0]![1]).toEqual({ context: { skipErrorToast: true } })
  })

  it(`a failed aside toasts Couldn't ask … and reports failed (the draft stays)`, async () => {
    mocks.send.mockRejectedValue(new Error(`Device offline`))
    await expect(dispatch(`claude`)(`/btw why?`)).resolves.toBe(`failed`)
    expect(mocks.toastActionError).toHaveBeenCalledTimes(1)
    expect(mocks.toastActionError.mock.calls[0]![0]).toBe(`Couldn't ask the side question`)
    expect(mocks.toastActionError.mock.calls[0]![1]).toMatchObject({
      description: `Device offline`,
    })
  })

  it(`hands anything else back as a message (null), without a call`, async () => {
    await expect(dispatch(`claude`)(`/compact`)).resolves.toBeNull()
    await expect(dispatch(`claude`)(`by the way`)).resolves.toBeNull()
    await expect(dispatch(`codex`)(`/btw why?`)).resolves.toBeNull()
    expect(mocks.send).not.toHaveBeenCalled()
  })
})

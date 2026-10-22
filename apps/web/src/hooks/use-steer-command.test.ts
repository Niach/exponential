import { renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  configure: vi.fn(),
  send: vi.fn(),
  toastActionError: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    sessions: {
      configure: { mutate: mocks.configure },
      send: { mutate: mocks.send },
    },
  },
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  toastActionError: mocks.toastActionError,
}))

import { effortLevel, useSteerCommand } from "@/hooks/use-steer-command"

// NEW-2: `/effort <level>` is the only steer command the web dispatches
// itself (`sessions.configure`); `/compact` and `/model <alias>` stay message
// text, and `/btw` is gone.
describe(`effortLevel`, () => {
  it(`reads the level of an /effort`, () => {
    expect(effortLevel(`/effort high`, `claude`)).toBe(`high`)
    expect(effortLevel(`  /EFFORT  Max  `, null)).toBe(`max`)
    expect(effortLevel(`/effort low`, `codex`)).toBe(`low`)
  })

  it(`leaves other commands, prose and a bare /effort alone`, () => {
    expect(effortLevel(`/compact`, `claude`)).toBeNull()
    expect(effortLevel(`/model opus`, `claude`)).toBeNull()
    expect(effortLevel(`set /effort high`, `claude`)).toBeNull()
    expect(effortLevel(`/effort`, `claude`)).toBeNull()
  })
})

describe(`useSteerCommand`, () => {
  beforeEach(() => {
    mocks.configure.mockReset()
    mocks.send.mockReset()
    mocks.toastActionError.mockReset()
  })

  const dispatch = (agent: string | null) =>
    renderHook(() => useSteerCommand({ sessionId: `s1`, agent })).result.current

  it(`sets /effort through sessions.configure and reports sent`, async () => {
    mocks.configure.mockResolvedValue({ ok: true })
    await expect(dispatch(`claude`)(`/effort high`)).resolves.toBe(`sent`)
    expect(mocks.configure).toHaveBeenCalledTimes(1)
    expect(mocks.configure.mock.calls[0]![0]).toEqual({ sessionId: `s1`, effort: `high` })
    // The hook raises its own toast: the generic mutation toast is skipped.
    expect(mocks.configure.mock.calls[0]![1]).toEqual({ context: { skipErrorToast: true } })
    expect(mocks.send).not.toHaveBeenCalled()
  })

  it(`a failed /effort toasts Couldn't set the effort and reports failed`, async () => {
    mocks.configure.mockRejectedValue(new Error(`Session ended`))
    await expect(dispatch(`claude`)(`/effort high`)).resolves.toBe(`failed`)
    expect(mocks.toastActionError).toHaveBeenCalledTimes(1)
    expect(mocks.toastActionError.mock.calls[0]![0]).toBe(`Couldn't set the effort`)
    expect(mocks.toastActionError.mock.calls[0]![1]).toMatchObject({
      description: `Session ended`,
    })
  })

  it(`hands anything else back as a message (null), without a call`, async () => {
    await expect(dispatch(`claude`)(`/compact`)).resolves.toBeNull()
    await expect(dispatch(`claude`)(`/model opus`)).resolves.toBeNull()
    await expect(dispatch(`claude`)(`/btw why?`)).resolves.toBeNull()
    await expect(dispatch(`claude`)(`by the way`)).resolves.toBeNull()
    expect(mocks.configure).not.toHaveBeenCalled()
    expect(mocks.send).not.toHaveBeenCalled()
  })
})

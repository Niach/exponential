import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { useAgentLogin } from "@/hooks/use-agent-login"

// EXP-1626: the sign-in loop names a long-lived Add's profile, exposes the
// `agent_login` id it tracks, and cancels it fire-and-forget.

const mocks = vi.hoisted(() => ({
  createCommand: vi.fn(),
  getCommand: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    devices: {
      createCommand: { mutate: mocks.createCommand },
      getCommand: { query: mocks.getCommand },
    },
  },
}))

beforeEach(() => {
  mocks.createCommand.mockReset()
  mocks.getCommand.mockReset()
  mocks.createCommand.mockResolvedValue({ id: `login-1` })
  mocks.getCommand.mockResolvedValue({ status: `pending`, result: null })
})

function hook(deviceId: string | undefined = `dev`) {
  return renderHook(
    ({ id }) => useAgentLogin({ deviceId: id, online: true, active: false }),
    { initialProps: { id: deviceId as string | undefined } }
  )
}

describe(`useAgentLogin (EXP-1626)`, () => {
  it(`sends a long-lived add's name and tracks the login command id`, async () => {
    const { result } = hook()
    expect(result.current.loginCommandIdFor(`claude`)).toBeNull()
    act(() => result.current.queueLogin(`claude`, false, { longLived: true, name: ` Work ` }))
    await waitFor(() => expect(result.current.loginCommandIdFor(`claude`)).toBe(`login-1`))
    expect(mocks.createCommand.mock.calls[0][0]).toEqual({
      deviceId: `dev`,
      kind: `agent_login`,
      agent: `claude`,
      switch: false,
      longLived: true,
      name: `Work`,
    })
  })

  it(`never names a re-login or the browser login`, async () => {
    const { result } = hook()
    act(() =>
      result.current.queueLogin(`claude`, false, { profileId: `p1`, longLived: true, name: `Work` })
    )
    act(() => result.current.queueLogin(`claude`, false, { longLived: false, name: `Work` }))
    await waitFor(() => expect(mocks.createCommand).toHaveBeenCalledTimes(2))
    for (const [input] of mocks.createCommand.mock.calls) {
      expect(input).not.toHaveProperty(`name`)
    }
  })

  it(`cancels on the device the login was queued on, swallowing errors`, async () => {
    const { result, rerender } = hook()
    act(() => result.current.queueLogin(`claude`, false, { longLived: true }))
    await waitFor(() => expect(result.current.loginCommandIdFor(`claude`)).toBe(`login-1`))
    // The dialog's target is gone by the time it closes.
    rerender({ id: undefined })
    mocks.createCommand.mockRejectedValueOnce(new Error(`offline`))
    act(() => result.current.cancelLogin(`claude`, `login-1`))
    expect(mocks.createCommand).toHaveBeenLastCalledWith(
      { deviceId: `dev`, kind: `agent_login_cancel`, agent: `claude`, loginCommandId: `login-1` },
      { context: { skipErrorToast: true } }
    )
    // A swallowed failure sets no error, and the login is no longer watched.
    await Promise.resolve()
    expect(result.current.stateFor(`claude`).error).toBe(``)
    expect(result.current.busy).toBe(false)
  })
})

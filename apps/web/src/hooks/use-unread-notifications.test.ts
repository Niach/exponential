import { beforeEach, describe, expect, it, vi } from "vitest"

// Mark all read ×4: the call carries the team scope (or none), raises its own
// failed-action toast with Retry (never the generic mutation toast).

const mutate = vi.fn()
const failedAction = vi.fn()
vi.mock(`@/lib/collections`, () => ({}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { notifications: { markAllRead: { mutate: (...args: unknown[]) => mutate(...args) } } },
}))
vi.mock(`@/lib/action-error`, () => ({
  SKIP_ERROR_TOAST: { context: { skipErrorToast: true } },
  failedAction: (...args: unknown[]) => failedAction(...args),
}))

const { markAllNotificationsRead } = await import(`./use-unread-notifications`)

describe(`markAllNotificationsRead`, () => {
  beforeEach(() => {
    mutate.mockReset()
    failedAction.mockReset()
  })

  it(`clears every team without a team, only that team's rows with one`, async () => {
    mutate.mockResolvedValue({ txId: 1 })
    markAllNotificationsRead()
    markAllNotificationsRead(`t-1`)
    await Promise.resolve()
    expect(mutate.mock.calls).toEqual([
      [undefined, { context: { skipErrorToast: true } }],
      [{ teamId: `t-1` }, { context: { skipErrorToast: true } }],
    ])
    expect(failedAction).not.toHaveBeenCalled()
  })

  it(`a failure toasts Couldn't mark all read with a Retry that re-runs the call`, async () => {
    const error = new Error(`Server said no`)
    mutate.mockRejectedValueOnce(error).mockResolvedValue({ txId: 2 })
    markAllNotificationsRead(`t-1`)
    await vi.waitFor(() => expect(failedAction).toHaveBeenCalledTimes(1))
    const [verb, thing, err, options] = failedAction.mock.calls[0]
    expect([verb, thing, err]).toEqual([`mark`, `all read`, error])
    options.retry()
    expect(mutate).toHaveBeenLastCalledWith({ teamId: `t-1` }, { context: { skipErrorToast: true } })
  })
})

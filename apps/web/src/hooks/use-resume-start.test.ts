import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1259 (start surfaces): a Resume / account-switch press mints ONE
// startId; the failure toast's Retry and the "never started" toast's Retry
// re-send that same id, a new press mints a new one. The deadline ends the
// pending state, the watch keeps following for `starting.watchMs`.

const h = vi.hoisted(() => ({
  sessions: [] as unknown[],
  mutate: vi.fn(),
  openSession: vi.fn(),
  toastActionError: vi.fn(),
  toastDismiss: vi.fn(),
  failedAction: vi.fn(),
}))

vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: (build: (query: unknown) => unknown) => {
    const query = { from: () => ({}) }
    if (build(query) === undefined) return { data: undefined }
    return { data: h.sessions }
  },
}))
vi.mock(`@/lib/collections`, () => ({ codingSessionCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { steer: { startSession: { mutate: h.mutate } } },
}))
vi.mock(`@/hooks/use-open-session`, () => ({
  useOpenSession: () => h.openSession,
}))
vi.mock(`@/lib/action-error`, () => ({
  SKIP_ERROR_TOAST: {},
  failedAction: h.failedAction,
}))
vi.mock(`@exp/ui`, () => ({
  toast: { dismiss: h.toastDismiss },
  toastActionError: h.toastActionError,
}))

import { useResumeStart } from "@/hooks/use-resume-start"

const sentIds = () =>
  h.mutate.mock.calls.map((call) => (call[0] as { startId: string }).startId)

function mount(canRetry?: (account: string | undefined) => boolean) {
  const onBeforeSend = vi.fn()
  const hook = renderHook(() =>
    useResumeStart({
      sessionId: `run-old`,
      userId: `me`,
      deviceId: `dev-1`,
      deadlineTitle: () => `buildbox never started this session`,
      failure: { verb: `resume`, thing: `the session` },
      canRetry,
      onBeforeSend,
    })
  )
  const sync = (...rows: unknown[]) => {
    h.sessions = rows
    hook.rerender()
  }
  return { ...hook, onBeforeSend, sync }
}

const flush = () => act(async () => {})

const resumedRow = (id: string) => ({
  id,
  userId: `me`,
  issueId: null,
  actionName: null,
  resumedFromId: `run-old`,
  startedReason: null,
  startedAt: new Date(Date.now()).toISOString(),
})

beforeEach(() => {
  vi.useFakeTimers()
  h.sessions = []
  h.mutate.mockReset()
  h.openSession.mockReset()
  h.toastActionError.mockReset()
  h.toastDismiss.mockReset()
  h.failedAction.mockReset()
})
afterEach(() => {
  vi.useRealTimers()
})

describe(`useResumeStart (EXP-1259)`, () => {
  it(`sends the resume with a startId and the account`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    const { result, onBeforeSend } = mount()
    act(() => result.current.resume(`work`))
    await flush()
    expect(h.mutate).toHaveBeenCalledTimes(1)
    expect(h.mutate.mock.calls[0]![0]).toMatchObject({
      resumeSessionId: `run-old`,
      deviceId: `dev-1`,
      account: `work`,
      startId: expect.stringMatching(/^[0-9a-f-]{36}$/),
    })
    expect(onBeforeSend).toHaveBeenCalledTimes(1)
    expect(result.current.pending).toEqual({ account: `work` })
  })

  it(`a failure's Retry re-sends the same startId; a new press mints a new one`, async () => {
    h.mutate.mockRejectedValueOnce(new Error(`relay timeout`))
    h.mutate.mockResolvedValue({ ok: true })
    const { result } = mount()
    act(() => result.current.resume())
    await flush()
    expect(result.current.pending).toBeNull()
    expect(h.failedAction).toHaveBeenCalledTimes(1)
    const retry = h.failedAction.mock.calls[0]![3].retry as () => void
    act(() => retry())
    await flush()
    expect(sentIds()).toHaveLength(2)
    expect(sentIds()[1]).toBe(sentIds()[0])
    act(() => vi.advanceTimersByTime(180_000))
    act(() => result.current.resume())
    await flush()
    expect(sentIds()).toHaveLength(3)
    expect(sentIds()[2]).not.toBe(sentIds()[0])
  })

  it(`the deadline toast's Retry reuses the id, and a late row still opens and clears it`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    h.toastActionError.mockReturnValue(`toast-1`)
    const { result, sync } = mount()
    act(() => result.current.resume())
    await flush()
    act(() => vi.advanceTimersByTime(180_000))
    expect(result.current.pending).toBeNull()
    expect(h.toastActionError).toHaveBeenCalledWith(
      `buildbox never started this session`,
      expect.objectContaining({ retry: expect.any(Function) })
    )
    const retry = h.toastActionError.mock.calls[0]![1].retry as () => void
    act(() => retry())
    await flush()
    expect(sentIds()).toHaveLength(2)
    expect(sentIds()[1]).toBe(sentIds()[0])
    act(() => vi.advanceTimersByTime(180_000))
    act(() => vi.advanceTimersByTime(10 * 60_000))
    sync(resumedRow(`run-new`))
    expect(h.openSession).toHaveBeenCalledTimes(1)
    expect(h.toastDismiss).toHaveBeenCalledWith(`toast-1`)
  })

  it(`stops following after starting.watchMs`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    const { result, sync } = mount()
    act(() => result.current.resume())
    await flush()
    act(() => vi.advanceTimersByTime(30 * 60_000))
    sync(resumedRow(`run-new`))
    expect(h.openSession).not.toHaveBeenCalled()
  })

  it(`a Retry the caller no longer allows sends nothing`, async () => {
    h.mutate.mockRejectedValueOnce(new Error(`refused`))
    const { result } = mount(() => false)
    act(() => result.current.resume(`work`))
    await flush()
    const retry = h.failedAction.mock.calls[0]![3].retry as () => void
    act(() => retry())
    await flush()
    expect(h.mutate).toHaveBeenCalledTimes(1)
  })

  it(`a second press while one is pending sends nothing`, async () => {
    h.mutate.mockImplementation(() => new Promise(() => {}))
    const { result } = mount()
    act(() => {
      result.current.resume()
      result.current.resume()
    })
    expect(h.mutate).toHaveBeenCalledTimes(1)
  })
})

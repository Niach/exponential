import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCClientError } from "@trpc/client"
import type { SteerDevice } from "@/lib/steer-devices"

// Final review B8: the post-send watch. A quiet watch stays quiet however
// slow the mutation, a send resolving after unmount arms nothing, the watch
// follows for `starting.watchMs` (30 min) and a late row clears the
// deadline's error; a quiet watch's gate can keep it from navigating.
// B6: a different startId supersedes the previous watch at the top of the
// send, the same startId keeps it, and a row created before the send never
// lands for it.

const h = vi.hoisted(() => ({
  sessions: [] as unknown[],
  mutate: vi.fn(),
  openSession: vi.fn(),
  toastError: vi.fn(),
  toastDismiss: vi.fn(),
  failedAction: vi.fn(),
}))

vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: (build: (query: unknown) => unknown) => {
    let alias = ``
    const query = {
      from: (source: Record<string, unknown>) => {
        alias = Object.keys(source)[0] ?? ``
        return {}
      },
    }
    if (build(query) === undefined) return { data: undefined }
    return { data: alias === `s` ? h.sessions : [] }
  },
}))
vi.mock(`@/lib/collections`, () => ({
  codingSessionCollection: {},
  deviceCollection: {},
  userCollection: {},
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    steer: { startSession: { mutate: h.mutate } },
    devices: { latestVersions: { query: () => Promise.resolve(null) } },
  },
}))
// EXP-1284 (review T11): an unhandled start lands through
// `useOpenStartedSession` (its thread, else the session page).
vi.mock(`@/hooks/use-open-started-session`, () => ({
  useOpenStartedSession: () => h.openSession,
}))
vi.mock(`@/hooks/use-open-session`, () => ({
  useOpenSession: () => h.openSession,
}))
vi.mock(`@/lib/action-error`, () => ({
  SKIP_ERROR_TOAST: {},
  failedAction: h.failedAction,
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@exp/ui")>()),
  toast: { error: h.toastError, dismiss: h.toastDismiss },
}))

import { useRemoteStart } from "@/hooks/use-remote-start"

const device: SteerDevice = {
  rowId: `row-1`,
  deviceId: `dev-1`,
  deviceLabel: `buildbox`,
  agents: [`claude`],
  caps: [],
  online: true,
  launchDefaults: undefined,
}
const action = { id: `act-1`, name: `Deploy`, teamId: `t1` }
const NEVER_STARTED = `buildbox never started this session`

// A synced action row, created (and started) now.
const row = (id: string) => ({
  id,
  userId: `me`,
  issueId: null,
  actionName: `Deploy`,
  startedAt: new Date(Date.now()).toISOString(),
  createdAt: new Date(Date.now()).toISOString(),
})

function mount() {
  const onOpened = vi.fn()
  const hook = renderHook(() =>
    useRemoteStart({ currentUserId: `me`, teamId: `t1`, onOpened })
  )
  // A synced row arrives: re-render so the watch reads it.
  const sync = (...rows: unknown[]) => {
    h.sessions = rows
    hook.rerender()
  }
  return { ...hook, onOpened, sync }
}

beforeEach(() => {
  vi.useFakeTimers()
  h.sessions = []
  h.mutate.mockReset()
  h.openSession.mockReset()
  h.toastError.mockReset()
  h.toastDismiss.mockReset()
  h.failedAction.mockReset()
})
afterEach(() => {
  vi.useRealTimers()
})

describe(`useRemoteStart watch`, () => {
  it(`toasts at the deadline, keeps following and a late row clears the toast`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    h.toastError.mockReturnValue(`toast-1`)
    const { result, sync, onOpened } = mount()
    await act(() => result.current.runAction(device, action, {} as never))
    expect(result.current.sentTo).toBe(`buildbox`)
    act(() => vi.advanceTimersByTime(180_000))
    expect(h.toastError).toHaveBeenCalledWith(NEVER_STARTED, expect.anything())
    expect(result.current.sentTo).toBeNull()
    act(() => vi.advanceTimersByTime(10 * 60_000))
    sync(row(`run-1`))
    expect(h.toastDismiss).toHaveBeenCalledWith(`toast-1`)
    expect(h.openSession).toHaveBeenCalledTimes(1)
    expect(onOpened).toHaveBeenCalledTimes(1)
    expect(result.current.openedId).toBe(`run-1`)
  })

  it(`stops following after 30 minutes`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    const { result, sync } = mount()
    await act(() => result.current.runAction(device, action, {} as never))
    act(() => vi.advanceTimersByTime(30 * 60_000))
    sync(row(`run-1`))
    expect(h.openSession).not.toHaveBeenCalled()
    expect(result.current.openedId).toBeNull()
  })

  it(`a watch quieted while the request is in flight never toasts`, async () => {
    let resolve: (value: unknown) => void = () => {}
    h.mutate.mockImplementation(() => new Promise((done) => (resolve = done)))
    const { result } = mount()
    let sent: Promise<void> = Promise.resolve()
    act(() => {
      sent = result.current.runAction(device, action, {} as never)
    })
    // The composer's 30 s deadline gives up before the mutation answers.
    act(() => vi.advanceTimersByTime(30_000))
    act(() => result.current.quietWatch())
    await act(async () => {
      resolve({ ok: true })
      await sent
    })
    act(() => vi.advanceTimersByTime(180_000))
    expect(h.toastError).not.toHaveBeenCalled()
  })

  it(`a send that resolves after unmount arms nothing`, async () => {
    let resolve: (value: unknown) => void = () => {}
    h.mutate.mockImplementation(() => new Promise((done) => (resolve = done)))
    const { result, unmount } = mount()
    let sent: Promise<void> = Promise.resolve()
    act(() => {
      sent = result.current.runAction(device, action, {} as never)
    })
    unmount()
    resolve({ ok: true })
    await sent
    vi.advanceTimersByTime(180_000)
    expect(h.toastError).not.toHaveBeenCalled()
  })

  it(`a quiet watch's gate keeps a late row from navigating`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    const { result, sync, onOpened } = mount()
    await act(() => result.current.runAction(device, action, {} as never))
    act(() => result.current.quietWatch(() => false))
    act(() => vi.advanceTimersByTime(5 * 60_000))
    sync(row(`run-1`))
    expect(result.current.openedId).toBe(`run-1`)
    expect(h.openSession).not.toHaveBeenCalled()
    expect(onOpened).not.toHaveBeenCalled()
    expect(h.toastError).not.toHaveBeenCalled()
  })

  it(`a different startId supersedes the previous watch at the top of the send`, async () => {
    h.mutate.mockResolvedValueOnce({ ok: true })
    const { result, sync } = mount()
    await act(() =>
      result.current.runAction(device, action, {} as never, undefined, undefined, undefined, `start-a`)
    )
    act(() => vi.advanceTimersByTime(30_000))
    act(() => result.current.quietWatch(() => true))
    act(() => vi.advanceTimersByTime(10_000))
    // The first start's run, created before the second send goes out.
    const late = row(`run-a`)
    act(() => vi.advanceTimersByTime(1_000))
    let resolve: (value: unknown) => void = () => {}
    h.mutate.mockImplementationOnce(() => new Promise((done) => (resolve = done)))
    let sent: Promise<void> = Promise.resolve()
    act(() => {
      sent = result.current.runAction(
        device,
        action,
        {} as never,
        undefined,
        undefined,
        undefined,
        `start-b`
      )
    })
    // It syncs while the new request is in flight: the old watch is gone.
    sync(late)
    expect(result.current.openedId).toBeNull()
    await act(async () => {
      resolve({ ok: true })
      await sent
    })
    // Same subject, started inside the skew window, but created before the
    // new send: never the new start's landing.
    sync(late)
    expect(result.current.openedId).toBeNull()
    expect(h.openSession).not.toHaveBeenCalled()
    act(() => vi.advanceTimersByTime(5_000))
    sync(late, row(`run-b`))
    expect(result.current.openedId).toBe(`run-b`)
    expect(h.openSession).toHaveBeenCalledTimes(1)
    expect(h.openSession.mock.calls[0]![0]).toMatchObject({ id: `run-b` })
  })

  it(`a re-send of the same startId keeps the watch and its first send's cutoff`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    const { result, sync, onOpened } = mount()
    await act(() =>
      result.current.runAction(device, action, {} as never, undefined, undefined, undefined, `start-a`)
    )
    act(() => vi.advanceTimersByTime(10_000))
    // The device created the run 10 s after the FIRST send; it syncs late.
    const original = row(`run-a`)
    act(() => vi.advanceTimersByTime(20_000))
    act(() => result.current.quietWatch(() => true))
    act(() => vi.advanceTimersByTime(5 * 60_000))
    await act(() =>
      result.current.runAction(device, action, {} as never, undefined, undefined, undefined, `start-a`)
    )
    sync(original)
    expect(result.current.openedId).toBe(`run-a`)
    expect(h.openSession).toHaveBeenCalledTimes(1)
    expect(onOpened).toHaveBeenCalledTimes(1)
  })

  it(`a row created before the send is never its run`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    const { result, sync } = mount()
    const earlier = row(`run-old`)
    act(() => vi.advanceTimersByTime(1_000))
    await act(() => result.current.runAction(device, action, {} as never))
    sync(earlier)
    expect(result.current.openedId).toBeNull()
    expect(h.openSession).not.toHaveBeenCalled()
  })
})

// EXP-1259 (start surfaces): a send without a caller startId is a new
// press-intent and mints one; its failure toast's Retry hands that id to the
// caller's re-submit of the same subject on the same device.
describe(`useRemoteStart startId (EXP-1259)`, () => {
  const sentIds = () =>
    h.mutate.mock.calls.map((call) => (call[0] as { startId?: string }).startId)

  it(`mints a startId per press when the caller passes none`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    const { result } = mount()
    await act(() => result.current.runAction(device, action, {} as never))
    await act(() => result.current.startIssues(device, {} as never, [`i-1`]))
    expect(sentIds()[0]).toMatch(/^[0-9a-f-]{36}$/)
    expect(sentIds()[1]).toMatch(/^[0-9a-f-]{36}$/)
    expect(sentIds()[1]).not.toBe(sentIds()[0])
  })

  it(`the failure toast's Retry re-sends the same startId, a new press a new one`, async () => {
    h.mutate.mockRejectedValueOnce(new Error(`relay timeout`))
    h.mutate.mockResolvedValue({ ok: true })
    const { result } = mount()
    let retry: (() => void) | undefined
    const press = (onRetry?: () => void) =>
      result.current.startIssues(
        device,
        {} as never,
        [`i-1`],
        undefined,
        onRetry
      )
    // The caller's own Retry re-submits the same subject, id-less.
    const resubmit = () => void press(resubmit)
    h.failedAction.mockImplementation(
      (_verb: string, _thing: string, _error: unknown, options?: { retry?: () => void }) => {
        retry = options?.retry
      }
    )
    await act(() => press(resubmit).catch(() => {}))
    expect(retry).toBeDefined()
    await act(async () => retry!())
    expect(sentIds()).toHaveLength(2)
    expect(sentIds()[1]).toBe(sentIds()[0])
    await act(() => press())
    expect(sentIds()[2]).not.toBe(sentIds()[0])
  })

  it(`a handed-over id never rides a different subject`, async () => {
    h.mutate.mockRejectedValueOnce(new Error(`relay timeout`))
    h.mutate.mockResolvedValue({ ok: true })
    const { result } = mount()
    let retry: (() => void) | undefined
    h.failedAction.mockImplementation(
      (_verb: string, _thing: string, _error: unknown, options?: { retry?: () => void }) => {
        retry = options?.retry
      }
    )
    const other = () =>
      void result.current.startIssues(device, {} as never, [`i-2`])
    await act(() =>
      result.current
        .startIssues(device, {} as never, [`i-1`], undefined, other)
        .catch(() => {})
    )
    await act(async () => retry!())
    expect(sentIds()).toHaveLength(2)
    expect(sentIds()[1]).not.toBe(sentIds()[0])
  })
})

// EXP-1284: a start into a THREAD carries `threadId` beside the batch's
// startId; the delivered id reaches `onSent` (the New session tab waits on
// THAT start) and `onOpened`, whose `true` means it landed the session
// itself (as the thread's tab), so the hook navigates nowhere.
describe(`useRemoteStart thread starts (EXP-1284)`, () => {
  it(`sends threadId + startId and hands the delivered id to onSent and onOpened`, async () => {
    h.mutate.mockImplementation(async (input: { startId: string }) => ({
      ok: true,
      startId: input.startId,
    }))
    const onSent = vi.fn()
    const onOpened = vi.fn(() => true)
    const hook = renderHook(() =>
      useRemoteStart({ currentUserId: `me`, teamId: `t1`, threadId: `th1`, onSent, onOpened })
    )
    await act(() => hook.result.current.runAction(device, action, {} as never))
    const sent = h.mutate.mock.calls[0]![0] as { threadId: string; startId: string }
    expect(sent.threadId).toBe(`th1`)
    expect(sent.startId).toEqual(expect.any(String))
    expect(onSent).toHaveBeenCalledWith({ startId: sent.startId })

    h.sessions = [row(`run-1`)]
    hook.rerender()
    expect(onOpened).toHaveBeenCalledWith(expect.objectContaining({ id: `run-1` }), {
      startId: sent.startId,
    })
    expect(h.openSession).not.toHaveBeenCalled()
  })

  it(`lands an unhandled start through the thread landing, naming the thread`, async () => {
    h.mutate.mockResolvedValue({ ok: true })
    const hook = renderHook(() =>
      useRemoteStart({ currentUserId: `me`, teamId: `t1`, threadId: `th1` })
    )
    await act(() => hook.result.current.startIssues(device, {} as never, [`i1`]))
    h.sessions = [{ ...row(`run-2`), issueId: `i1`, actionName: null }]
    hook.rerender()
    expect(h.openSession).toHaveBeenCalledWith(
      expect.objectContaining({ id: `run-2` }),
      expect.objectContaining({ knownThreadId: `th1` })
    )
  })
})

// Recheck R3 (feedback-states.json `starting.maybeDelivered`): a failed send
// whose fate is unknown toasts with Retry AND follows quietly for
// `starting.watchMs`; a definite error only toasts.
describe(`useRemoteStart maybe-delivered (recheck R3)`, () => {
  const serverError = (code: string, httpStatus: number) => {
    const error = new TRPCClientError(`Steer relay error (${httpStatus})`)
    ;(error as { data?: unknown }).data = { code, httpStatus }
    return error
  }
  const transport = () =>
    new TRPCClientError(`Failed to fetch`, { cause: new TypeError(`Failed to fetch`) })

  for (const [name, error] of [
    [`a transport error`, transport],
    [`the relay timeout`, () => serverError(`GATEWAY_TIMEOUT`, 504)],
  ] as const) {
    it(`${name}: toast + Retry, a quiet follow, a late row opens and clears the toast`, async () => {
      h.mutate.mockRejectedValueOnce(error())
      h.failedAction.mockReturnValue(`toast-3`)
      const { result, sync, onOpened } = mount()
      await act(() =>
        result.current.runAction(device, action, {} as never, undefined, undefined, vi.fn()).catch(() => {})
      )
      expect(h.failedAction).toHaveBeenCalledTimes(1)
      expect(h.failedAction.mock.calls[0]![3].retry).toEqual(expect.any(Function))
      // Quiet: no "sent" caption and no "never started" toast.
      expect(result.current.sentTo).toBeNull()
      act(() => vi.advanceTimersByTime(20 * 60_000))
      expect(h.toastError).not.toHaveBeenCalled()
      sync(row(`run-1`))
      expect(result.current.openedId).toBe(`run-1`)
      expect(h.openSession).toHaveBeenCalledTimes(1)
      expect(onOpened).toHaveBeenCalledTimes(1)
      expect(h.toastDismiss).toHaveBeenCalledWith(`toast-3`)
    })
  }

  it(`a quiet follow ends after starting.watchMs`, async () => {
    h.mutate.mockRejectedValueOnce(transport())
    const { result, sync } = mount()
    await act(() =>
      result.current.runAction(device, action, {} as never).catch(() => {})
    )
    act(() => vi.advanceTimersByTime(30 * 60_000))
    sync(row(`run-1`))
    expect(result.current.openedId).toBeNull()
  })

  for (const [name, error] of [
    [`a 4xx`, () => serverError(`PRECONDITION_FAILED`, 412)],
    [`a 5xx with another error body`, () => serverError(`INTERNAL_SERVER_ERROR`, 500)],
  ] as const) {
    it(`${name}: toast + Retry, no follower`, async () => {
      h.mutate.mockRejectedValueOnce(error())
      const { result, sync } = mount()
      await act(() =>
        result.current.runAction(device, action, {} as never, undefined, undefined, vi.fn()).catch(() => {})
      )
      expect(h.failedAction.mock.calls[0]![3].retry).toEqual(expect.any(Function))
      sync(row(`run-1`))
      expect(result.current.openedId).toBeNull()
      expect(h.openSession).not.toHaveBeenCalled()
    })
  }
})

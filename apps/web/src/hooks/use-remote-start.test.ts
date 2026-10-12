import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerDevice } from "@/lib/steer-devices"

// Final review B8: the post-send watch. A quiet watch stays quiet however
// slow the mutation, a send resolving after unmount arms nothing, the watch
// follows for `starting.watchMs` (30 min) and a late row clears the
// deadline's error; a quiet watch's gate can keep it from navigating.

const h = vi.hoisted(() => ({
  sessions: [] as unknown[],
  mutate: vi.fn(),
  openSession: vi.fn(),
  toastError: vi.fn(),
  toastDismiss: vi.fn(),
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
vi.mock(`@/hooks/use-open-session`, () => ({
  useOpenSession: () => h.openSession,
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

const row = (id: string) => ({
  id,
  userId: `me`,
  issueId: null,
  actionName: `Deploy`,
  startedAt: new Date(Date.now()).toISOString(),
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
})

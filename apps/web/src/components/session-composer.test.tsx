import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SessionRow } from "@/lib/session-tree"

// EXP-1323 (#EXP-1330): THE session composer dispatches `/btw <question>`
// as an ASIDE child (`sessions.send({parentId, text, aside})`) — never as a
// message into the session — exactly as the IDE, iOS and Android do; every
// other draft is the ordinary `sessions.send({sessionId})`. The relay store,
// the synced row and tRPC are stubbed: this pins the SEND path.

const mocks = vi.hoisted(() => ({
  send: vi.fn(),
  toastActionError: vi.fn(),
  toastError: vi.fn(),
  row: null as unknown,
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    sessions: {
      send: { mutate: mocks.send },
      stop: { mutate: vi.fn() },
    },
  },
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})
vi.mock(`@/lib/collections`, () => ({
  deviceCollection: {},
  messageCollection: { utils: { awaitTxId: () => Promise.resolve() } },
}))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useSessionRow: () => ({ row: mocks.row, isReady: true }),
}))
vi.mock(`@/lib/steer-session-store`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/steer-session-store")>()
  // No relay state: the composer reads the synced row alone.
  const snapshot = { config: null, usage: null, contextLayout: null }
  return {
    ...actual,
    acquireSteerSession: () => ({
      subscribe: () => () => {},
      getSnapshot: () => snapshot,
      connect: () => () => {},
      noteSessionStatus: () => {},
    }),
  }
})
vi.mock(`@/lib/storage/issue-image-upload`, () => ({
  uploadTeamSessionImageFile: vi.fn(),
}))
vi.mock(`@exp/ui`, async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    useIsMobile: () => false,
    toastActionError: mocks.toastActionError,
    toast: { ...(actual.toast as object), error: mocks.toastError },
  }
})

const { SessionComposer } = await import(`./session-composer`)

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const sessionRow = (over: Partial<SessionRow> = {}) =>
  ({
    id: `s1`,
    teamId: `t1`,
    parentId: null,
    rootId: `s1`,
    kind: `thread`,
    userId: `u1`,
    deviceId: null,
    status: `idle`,
    busy: false,
    needsInput: false,
    blocked: null,
    shared: false,
    dispatch: false,
    cli: `claude`,
    model: `opus`,
    ...over,
  }) as unknown as SessionRow

function renderComposer() {
  render(<SessionComposer teamId="t1" sessionId="s1" placeholder="Reply…" />)
  return screen.getByPlaceholderText(`Reply…`) as HTMLTextAreaElement
}

function type(field: HTMLTextAreaElement, value: string) {
  act(() => {
    fireEvent.change(field, { target: { value } })
  })
}

function pressEnter(field: HTMLTextAreaElement) {
  act(() => {
    fireEvent.keyDown(field, { key: `Enter` })
  })
}

beforeEach(() => {
  mocks.send.mockReset()
  mocks.toastActionError.mockReset()
  mocks.toastError.mockReset()
  mocks.row = sessionRow()
})

describe(`SessionComposer /btw`, () => {
  it(`lists /btw in the slash menu of a claude session`, () => {
    const field = renderComposer()
    type(field, `/bt`)
    expect(screen.getByText(`/btw`)).toBeTruthy()
  })

  it(`sends the question as an aside child, never as a message, and clears the draft`, async () => {
    mocks.send.mockResolvedValue({ sessionId: `s2`, txid: 1 })
    const field = renderComposer()
    type(field, `/btw what does useCart return?`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(mocks.send.mock.calls[0]![0]).toEqual({
      teamId: `t1`,
      parentId: `s1`,
      text: `what does useCart return?`,
      aside: true,
    })
    await waitFor(() => expect(field.value).toBe(``))
  })

  it(`toasts Couldn't ask and keeps the draft when the aside fails`, async () => {
    mocks.send.mockRejectedValue(new Error(`Device offline`))
    const field = renderComposer()
    type(field, `/btw why?`)
    pressEnter(field)
    await waitFor(() => expect(mocks.toastActionError).toHaveBeenCalledTimes(1))
    expect(mocks.toastActionError.mock.calls[0]![0]).toBe(`Couldn't ask the side question`)
    expect(field.value).toBe(`/btw why?`)
    expect(mocks.toastError).not.toHaveBeenCalled()
  })

  it(`a codex session has no /btw: the text rides as a message`, async () => {
    mocks.row = sessionRow({ cli: `codex` })
    mocks.send.mockResolvedValue({ txid: 1 })
    const field = renderComposer()
    type(field, `/btw why?`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(mocks.send.mock.calls[0]![0]).toEqual({
      teamId: `t1`,
      sessionId: `s1`,
      text: `/btw why?`,
      attachmentIds: undefined,
    })
  })

  it(`an ordinary draft is the ordinary message`, async () => {
    mocks.send.mockResolvedValue({ txid: 1 })
    const field = renderComposer()
    type(field, `Ship it`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(mocks.send.mock.calls[0]![0]).toMatchObject({ sessionId: `s1`, text: `Ship it` })
    expect(mocks.send.mock.calls[0]![0]).not.toHaveProperty(`aside`)
  })
})

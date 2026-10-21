import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SessionRow } from "@/lib/session-tree"

// NEW-2: THE session composer's `/` menu = the contract builtins (compact,
// model, effort) and the team's prompts; `/effort <level>` rides
// `sessions.configure`, a picked prompt's body (`prompts.get`) lands in the
// draft, every other draft is the ordinary `sessions.send({sessionId})`. The
// relay store, the synced row and tRPC are stubbed: this pins the SEND path.
//
// EXP-1334: the ONE composer — the "+" is the options menu (never the file
// chooser), mid-session options ride `sessions.configure`, and the options
// line under the card carries the model pin (a pick = `/model <alias>`,
// shown at once) and the Plan switch.

const mocks = vi.hoisted(() => ({
  send: vi.fn(),
  configure: vi.fn(),
  toastActionError: vi.fn(),
  toastError: vi.fn(),
  awaitTxId: vi.fn(),
  stop: vi.fn(),
  row: null as unknown,
  // EXP-1342: the store's draft + the queue a Stop hands back.
  draft: { text: `` },
  queued: [] as string[],
  listeners: new Set<() => void>(),
  // EXP-1211: the session's synced messages (the quick replies' source).
  messages: [] as unknown[],
  sendReply: vi.fn(),
  dismissReplies: vi.fn(),
  promptGet: vi.fn(),
  prompts: [] as { id: string; name: string; icon: string | null; sortOrder: number }[],
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    sessions: {
      send: { mutate: mocks.send },
      configure: { mutate: mocks.configure },
      mcpPicks: { query: () => Promise.resolve({ mcpServerIds: [] }) },
      stop: { mutate: mocks.stop },
      sendReply: { mutate: mocks.sendReply },
      dismissReplies: { mutate: mocks.dismissReplies },
    },
    prompts: { get: { query: mocks.promptGet } },
  },
}))
vi.mock(`@/hooks/use-prompts`, () => ({
  usePrompts: () => ({ prompts: mocks.prompts, isReady: true }),
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})
vi.mock(`@/lib/collections`, () => ({
  deviceCollection: {},
  messageCollection: { utils: { awaitTxId: () => Promise.resolve() } },
  sessionCollection: {
    utils: { awaitTxId: (...args: unknown[]) => mocks.awaitTxId(...args) as Promise<void> },
  },
}))
vi.mock(`@/hooks/use-mcp-servers`, () => ({
  useMcpServers: () => ({ servers: [] }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamById: () => ({ slug: `acme` }),
}))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useSessionRow: () => ({ row: mocks.row, isReady: true }),
  useSessionMessages: () => mocks.messages,
}))
vi.mock(`@/lib/steer-session-store`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/steer-session-store")>()
  // No relay state: the composer reads the synced row alone.
  const snapshot = { config: null, usage: null, contextLayout: null }
  return {
    ...actual,
    acquireSteerSession: () => ({
      subscribe: (listener: () => void) => {
        mocks.listeners.add(listener)
        return () => mocks.listeners.delete(listener)
      },
      getSnapshot: () => snapshot,
      getDraftSnapshot: () => mocks.draft,
      connect: () => () => {},
      noteSessionStatus: () => {},
      noteStopped: () => {
        if (mocks.queued.length === 0) return false
        mocks.draft = { text: actual.mergeHeldIntoDraft(mocks.draft.text, mocks.queued) }
        mocks.queued = []
        for (const listener of mocks.listeners) listener()
        return true
      },
      takeDraftText: () => {
        const text = mocks.draft.text
        mocks.draft = { text: `` }
        return text
      },
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
  mocks.configure.mockReset()
  mocks.awaitTxId.mockReset()
  mocks.awaitTxId.mockResolvedValue(undefined)
  mocks.toastActionError.mockReset()
  mocks.toastError.mockReset()
  mocks.row = sessionRow()
  mocks.stop.mockReset()
  mocks.stop.mockResolvedValue({ txid: 1 })
  mocks.draft = { text: `` }
  mocks.queued = []
  mocks.listeners.clear()
  mocks.messages = []
  mocks.sendReply.mockReset()
  mocks.dismissReplies.mockReset()
  mocks.promptGet.mockReset()
  mocks.prompts = []
})

describe(`SessionComposer / menu (NEW-2)`, () => {
  it(`lists the builtins, then the team's prompts; never /btw or /clear`, () => {
    mocks.prompts = [{ id: `p1`, name: `Release train`, icon: null, sortOrder: 1 }]
    const field = renderComposer()
    type(field, `/`)
    expect(screen.getByText(`/compact`)).toBeTruthy()
    expect(screen.getByText(`/model`)).toBeTruthy()
    expect(screen.getByText(`/effort`)).toBeTruthy()
    expect(screen.getByText(`Release train`)).toBeTruthy()
    expect(screen.queryByText(`/btw`)).toBeNull()
    expect(screen.queryByText(`/clear`)).toBeNull()
  })

  it(`a picked prompt's body replaces the draft; nothing is sent`, async () => {
    mocks.prompts = [{ id: `p1`, name: `Release train`, icon: null, sortOrder: 1 }]
    mocks.promptGet.mockResolvedValue({ prompt: { id: `p1`, body: `Cut the release.` } })
    const field = renderComposer()
    type(field, `/rel`)
    pressEnter(field)
    await waitFor(() => expect(field.value).toBe(`Cut the release.`))
    expect(mocks.promptGet).toHaveBeenCalledWith({ id: `p1` })
    expect(mocks.send).not.toHaveBeenCalled()
  })

  it(`a failed prompt fetch toasts and keeps the draft`, async () => {
    mocks.prompts = [{ id: `p1`, name: `Release train`, icon: null, sortOrder: 1 }]
    mocks.promptGet.mockRejectedValue(new Error(`gone`))
    const field = renderComposer()
    type(field, `/rel`)
    pressEnter(field)
    await waitFor(() => expect(mocks.toastActionError).toHaveBeenCalledTimes(1))
    expect(mocks.toastActionError.mock.calls[0]![0]).toBe(`Couldn't load Release train`)
    expect(field.value).toBe(`/rel`)
  })

  it(`/effort sets the effort through sessions.configure, never a message`, async () => {
    mocks.configure.mockResolvedValue({ txid: 1 })
    const field = renderComposer()
    type(field, `/effort high`)
    pressEnter(field)
    await waitFor(() => expect(mocks.configure).toHaveBeenCalledTimes(1))
    expect(mocks.configure.mock.calls[0]![0]).toEqual({ sessionId: `s1`, effort: `high` })
    expect(mocks.send).not.toHaveBeenCalled()
    await waitFor(() => expect(field.value).toBe(``))
  })

  it(`/model rides as message text`, async () => {
    mocks.send.mockResolvedValue({ txid: 1 })
    const field = renderComposer()
    type(field, `/model sonnet`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(mocks.send.mock.calls[0]![0]).toMatchObject({ sessionId: `s1`, text: `/model sonnet` })
    expect(mocks.configure).not.toHaveBeenCalled()
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

// Radix opens a dropdown on the trigger's pointer DOWN.
function pointerOpen(element: HTMLElement) {
  act(() => {
    fireEvent.pointerDown(element, { button: 0, ctrlKey: false, pointerType: `mouse` })
  })
}

describe(`SessionComposer, the one composer (EXP-1334)`, () => {
  it(`the "+" opens the options menu, never the file chooser, and no subject rows`, () => {
    const click = vi.spyOn(HTMLInputElement.prototype, `click`)
    renderComposer()
    pointerOpen(screen.getByTestId(`agent-composer-plus-button`))
    expect(click).not.toHaveBeenCalled()
    const menu = screen.getByTestId(`agent-composer-menu`)
    expect(menu.querySelector(`[data-testid="agent-composer-menu-effort"]`)).toBeTruthy()
    expect(menu.querySelector(`[data-testid="agent-composer-menu-ultracode"]`)).toBeTruthy()
    expect(menu.querySelector(`[data-testid="agent-composer-menu-implement-issue"]`)).toBeNull()
    expect(menu.querySelector(`[data-testid="agent-composer-menu-run-action"]`)).toBeNull()
    click.mockRestore()
  })

  it(`carries the inline edit tools in the card`, () => {
    renderComposer()
    for (const name of [`Add image`, `Attach files`, `Insert issue reference`, `Insert emoji`]) {
      expect(screen.getByRole(`button`, { name })).toBeTruthy()
    }
  })

  it(`a model pick sends /model and reads as picked at once`, async () => {
    mocks.send.mockResolvedValue({ txid: 1 })
    renderComposer()
    const pin = screen.getByTestId(`session-composer-model`)
    expect(pin.textContent).toContain(`Opus`)
    act(() => {
      fireEvent.click(pin.querySelector(`button`)!)
    })
    act(() => {
      fireEvent.click(document.querySelector(`[data-value="sonnet"]`)!)
    })
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(mocks.send.mock.calls[0]![0]).toEqual({
      teamId: `t1`,
      sessionId: `s1`,
      text: `/model sonnet`,
    })
    expect(screen.getByTestId(`session-composer-model`).dataset.model).toBe(`sonnet`)
  })

  it(`a refused model pick toasts Couldn't switch the model and reverts`, async () => {
    mocks.send.mockRejectedValue(new Error(`Device offline`))
    renderComposer()
    act(() => {
      fireEvent.click(screen.getByTestId(`session-composer-model`).querySelector(`button`)!)
    })
    act(() => {
      fireEvent.click(document.querySelector(`[data-value="haiku"]`)!)
    })
    await waitFor(() => expect(mocks.toastActionError).toHaveBeenCalledTimes(1))
    expect(mocks.toastActionError.mock.calls[0]![0]).toBe(`Couldn't switch the model`)
    expect(screen.getByTestId(`session-composer-model`).dataset.model).toBe(`opus`)
  })

  it(`the Plan switch changes the live session through sessions.configure`, async () => {
    mocks.configure.mockResolvedValue({ txid: 7 })
    renderComposer()
    const plan = screen.getByTestId(`session-composer-plan`)
    expect(plan.getAttribute(`aria-checked`)).toBe(`false`)
    act(() => {
      fireEvent.click(plan)
    })
    await waitFor(() => expect(mocks.configure).toHaveBeenCalledTimes(1))
    expect(mocks.configure.mock.calls[0]![0]).toEqual({ sessionId: `s1`, planMode: true })
  })

  it(`a Plan pick outlives a slow sync and settles once the row lands`, async () => {
    // The row never syncs inside the wait: the pick stays (no flip back to
    // the stale row), then the synced row says the same and the pick goes.
    mocks.configure.mockResolvedValue({ txid: 7 })
    mocks.awaitTxId.mockRejectedValue(new Error(`timeout`))
    const { rerender } = render(<SessionComposer teamId="t1" sessionId="s1" placeholder="Reply…" />)
    act(() => {
      fireEvent.click(screen.getByTestId(`session-composer-plan`))
    })
    await waitFor(() => expect(mocks.awaitTxId).toHaveBeenCalledTimes(1))
    // Drain the configure chain (the rejected wait and whatever follows).
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
    expect(screen.getByTestId(`session-composer-plan`).getAttribute(`aria-checked`)).toBe(`true`)
    mocks.row = sessionRow({ planMode: true })
    rerender(<SessionComposer teamId="t1" sessionId="s1" placeholder="Reply…" />)
    expect(screen.getByTestId(`session-composer-plan`).getAttribute(`aria-checked`)).toBe(`true`)
    expect(mocks.toastActionError).not.toHaveBeenCalled()
  })

  it(`a second model pick while one is in flight sends nothing`, async () => {
    mocks.send.mockResolvedValue({ txid: 1 })
    renderComposer()
    const pick = (alias: string) => {
      act(() => {
        fireEvent.click(screen.getByTestId(`session-composer-model`).querySelector(`button`)!)
      })
      act(() => {
        fireEvent.click(document.querySelector(`[data-value="${alias}"]`)!)
      })
    }
    pick(`sonnet`)
    pick(`haiku`)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(mocks.send.mock.calls[0]![0]).toEqual({ teamId: `t1`, sessionId: `s1`, text: `/model sonnet` })
    expect(screen.getByTestId(`session-composer-model`).dataset.model).toBe(`sonnet`)
  })

  it(`a failed configure toasts and drops the optimistic pick`, async () => {
    mocks.configure.mockRejectedValue(new Error(`nope`))
    renderComposer()
    const plan = screen.getByTestId(`session-composer-plan`)
    act(() => {
      fireEvent.click(plan)
    })
    await waitFor(() => expect(mocks.toastActionError).toHaveBeenCalledTimes(1))
    expect(mocks.toastActionError.mock.calls[0]![0]).toBe(`Couldn't change the session options`)
    expect(screen.getByTestId(`session-composer-plan`).getAttribute(`aria-checked`)).toBe(`false`)
  })

  it(`an ended session cannot change its model or plan`, () => {
    mocks.row = sessionRow({ status: `ended` })
    renderComposer()
    expect(
      (screen.getByTestId(`session-composer-model`).querySelector(`button`) as HTMLButtonElement)
        .disabled
    ).toBe(true)
    expect((screen.getByTestId(`session-composer-plan`) as HTMLButtonElement).disabled).toBe(true)
  })
})

// EXP-1342: Stop hands the unread messages back to the composer (×4): the
// queued text lands in the field, the turn stops through sessions.stop.
describe(`SessionComposer Stop (EXP-1342)`, () => {
  it(`Stop puts the queued text back into the field`, async () => {
    mocks.row = sessionRow({ status: `running`, busy: true })
    mocks.queued = [`first`, `second`]
    render(<SessionComposer teamId="t1" sessionId="s1" placeholder="Reply…" busy canStop />)
    const field = screen.getByPlaceholderText(`Reply…`) as HTMLTextAreaElement
    await act(async () => {
      fireEvent.click(screen.getByRole(`button`, { name: `Stop` }))
    })
    expect(mocks.stop).toHaveBeenCalledWith({ sessionId: `s1` }, undefined)
    await waitFor(() => expect(field.value).toBe(`first\nsecond`))
    expect(mocks.draft.text).toBe(``)
  })
})

describe(`SessionComposer quick replies (EXP-1211)`, () => {
  const repliesMessage = (items: unknown[]) => ({
    id: `m2`,
    sessionId: `s1`,
    rootId: `s1`,
    teamId: `t1`,
    authorKind: `agent`,
    authorId: null,
    authorSessionId: `s1`,
    body: ``,
    card: { kind: `replies`, items },
    answeredAt: null,
    consumedAt: `2026-10-17T10:00:01.000Z`,
    createdAt: `2026-10-17T10:00:01.000Z`,
    updatedAt: `2026-10-17T10:00:01.000Z`,
  })

  it(`shows the open replies above the card and a tap sends that reply`, async () => {
    mocks.messages = [
      repliesMessage([
        { id: `a`, text: `Raise the version numbers` },
        { id: `b`, text: `Open the PR`, sent: true },
      ]),
    ]
    mocks.sendReply.mockResolvedValue({ sessionId: `s1`, txid: 1 })
    renderComposer()
    const pills = screen.getAllByTestId(`thread-reply`)
    expect(pills.map((pill) => pill.textContent)).toEqual([`Raise the version numbers`])
    fireEvent.click(pills[0]!)
    await waitFor(() =>
      expect(mocks.sendReply).toHaveBeenCalledWith({ messageId: `m2`, itemId: `a` }, expect.anything())
    )
    expect(mocks.send).not.toHaveBeenCalled()
  })

  it(`the dismiss stamps the card; no replies card = no row`, async () => {
    mocks.messages = [repliesMessage([{ id: `a`, text: `Ship it` }])]
    mocks.dismissReplies.mockResolvedValue({ txid: 1 })
    const { unmount } = render(<SessionComposer teamId="t1" sessionId="s1" placeholder="Reply…" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Dismiss quick replies` }))
    await waitFor(() =>
      expect(mocks.dismissReplies).toHaveBeenCalledWith({ messageId: `m2` }, expect.anything())
    )
    unmount()
    mocks.messages = []
    renderComposer()
    expect(screen.queryByTestId(`thread-replies`)).toBeNull()
  })
})

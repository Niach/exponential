import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
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
// line under the card carries the model pin (a pick = `sessions.configure {model}`,
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
  // #EXP-1535: THE session account switch (its own tests pin the rules).
  accountSwitch: null as unknown,
  switchTo: vi.fn(),
  // #EXP-1715: the agent behind the root + the viewer's teams.
  agent: null as unknown,
  teams: [] as { id: string; name: string; iconUrl: string | null }[],
  // #EXP-1793: the team's MCP servers + the conversation's pick.
  mcpServers: [] as unknown[],
  mcpPicks: vi.fn(),
}))

// #FEED-219: the composer field as a plain field (no drafts shape here).
vi.mock(`@/hooks/use-composer-draft`, () => import(`@/test/composer-draft-plain`))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    sessions: {
      send: { mutate: mocks.send },
      configure: { mutate: mocks.configure },
      mcpPicks: { query: mocks.mcpPicks },
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
vi.mock(`@/components/session-account-switch`, () => ({
  useSessionAccountSwitch: () =>
    mocks.accountSwitch ?? {
      canSteer: false,
      deviceOnline: false,
      blocked: null,
      switching: false,
      options: [],
      value: null,
      switchTo: mocks.switchTo,
    },
}))
vi.mock(`@/hooks/use-mcp-servers`, () => ({
  useMcpServers: () => ({ servers: mocks.mcpServers }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamById: () => ({ slug: `acme` }),
  useTeamMemberships: () => ({ memberships: [], myTeams: mocks.teams }),
}))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: { user: { id: `u1` } } }) }))
vi.mock(`@/hooks/use-agent-row`, () => ({ useAgentRow: () => ({ agent: mocks.agent, isReady: true }) }))
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
    agentId: null,
    userId: `u1`,
    deviceId: null,
    status: `idle`,
    busy: false,
    needsInput: false,
    blocked: null,
    shared: false,
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
  mocks.agent = null
  mocks.teams = []
  mocks.mcpServers = []
  mocks.mcpPicks.mockReset()
  mocks.mcpPicks.mockResolvedValue({ mcpServerIds: [] })
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

// #FEED-204 (apps.json EDITOR DEVICE): an existing editor root keeps
// `{sessionId}`; only an ended root moving elsewhere sends `{appId, deviceId}`.
describe(`SessionComposer app editor device (#FEED-204)`, () => {
  const sendFrom = async (moveDevice: boolean) => {
    mocks.send.mockResolvedValue({ txid: 1, sessionId: `s1` })
    render(<SessionComposer teamId="t1" sessionId="s1" appId="a1" deviceId="mac" moveDevice={moveDevice} placeholder="Reply…" />)
    const field = screen.getByPlaceholderText(`Reply…`) as HTMLTextAreaElement
    type(field, `Ship it`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    return mocks.send.mock.calls[0]![0]
  }

  it(`a root staying on its device gets a plain message`, async () => {
    const input = await sendFrom(false)
    expect(input).toMatchObject({ sessionId: `s1`, text: `Ship it` })
    expect(input).not.toHaveProperty(`deviceId`)
  })

  it(`an ended root moving sends the app form with the device`, async () => {
    const input = await sendFrom(true)
    expect(input).toMatchObject({ appId: `a1`, deviceId: `mac`, text: `Ship it` })
    expect(input).not.toHaveProperty(`sessionId`)
  })
})

// #EXP-1729 (apps.json `editorHeader`, composer-menu.json "an app editor's
// composer, always"): Device, Account and Model; with no root or an ENDED
// one the held picks ride the send, a LIVE root never carries them.
describe(`SessionComposer app editor runner (#EXP-1729)`, () => {
  const held = {
    account: {
      value: `claude:a`,
      options: [
        { key: `claude:a`, agent: `claude` as const, email: `a@example.com` },
        { key: `claude:b`, agent: `claude` as const, email: `b@example.com` },
      ],
      onChange: vi.fn(),
    },
    model: { value: `opus`, models: [{ value: `opus`, label: `Opus` }], onChange: vi.fn() },
    send: { cli: `claude`, account: `a`, model: `opus` },
  }
  const editor = (sessionId: string | null) =>
    render(
      <SessionComposer
        teamId="t1"
        sessionId={sessionId}
        appId="a1"
        deviceId="mac"
        deviceMenu={{ value: `mac`, devices: [{ id: `mac`, name: `Mac` }], onChange: vi.fn() }}
        heldRunner={held}
        placeholder="Reply…"
      />
    )
  const sendFrom = async (sessionId: string | null) => {
    mocks.send.mockResolvedValue({ txid: 1, sessionId: `s1` })
    editor(sessionId)
    const field = screen.getByPlaceholderText(`Reply…`) as HTMLTextAreaElement
    type(field, `Ship it`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    return mocks.send.mock.calls[0]![0]
  }

  it(`the "+" carries Device, Account and Model`, () => {
    mocks.row = null
    editor(null)
    pointerOpen(screen.getByTestId(`agent-composer-plus-button`))
    const ids = [...screen.getByTestId(`agent-composer-menu`).querySelectorAll(`[data-testid^="agent-composer-menu-"]`)].map(
      (row) => row.getAttribute(`data-testid`)!.replace(`agent-composer-menu-`, ``)
    )
    expect(ids).toEqual([`add-file`, `implement-issue`, `device`, `account`, `model`])
    expect(screen.getByTestId(`agent-composer-menu-account`).textContent).toContain(`a@example.com`)
  })

  it(`no root: the first send carries the picks`, async () => {
    mocks.row = null
    const input = await sendFrom(null)
    expect(input).toMatchObject({ appId: `a1`, deviceId: `mac`, cli: `claude`, account: `a`, model: `opus` })
  })

  it(`an ended root: the revive carries them`, async () => {
    mocks.row = sessionRow({ status: `ended` })
    const input = await sendFrom(`s1`)
    expect(input).toMatchObject({ sessionId: `s1`, account: `a`, model: `opus` })
  })

  it(`a live root: a plain message`, async () => {
    mocks.row = sessionRow({ status: `idle` })
    const input = await sendFrom(`s1`)
    expect(input).toMatchObject({ sessionId: `s1`, text: `Ship it` })
    expect(input).not.toHaveProperty(`account`)
    expect(input).not.toHaveProperty(`model`)
  })
})

// #EXP-1715 (composer-menu.json `teamsPick`): the viewer's Assistant's
// chat carries its Teams pick on every send (null = every team), seeded from
// the conversation root; any other chat never sends one.
describe(`SessionComposer Teams pick (#EXP-1715)`, () => {
  it(`the Assistant's chat sends its stored pick; another chat sends none`, async () => {
    mocks.send.mockResolvedValue({ txid: 1 })
    mocks.teams = [
      { id: `t1`, name: `Acme`, iconUrl: null },
      { id: `t2`, name: `Feedback`, iconUrl: null },
    ]
    mocks.agent = { id: `a1`, role: `assistant`, ownerId: `u1` }
    mocks.row = sessionRow({ agentId: `a1`, scopeTeamIds: [`t2`] } as never)
    const field = renderComposer()
    expect(screen.getByTestId(`agent-composer-chip-team-t2`)).toBeTruthy()
    type(field, `Only Feedback`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(mocks.send.mock.calls[0]![0]).toMatchObject({ sessionId: `s1`, scopeTeamIds: [`t2`] })
  })

  it(`a team agent's chat never carries a pick`, async () => {
    mocks.send.mockResolvedValue({ txid: 1 })
    mocks.teams = [
      { id: `t1`, name: `Acme`, iconUrl: null },
      { id: `t2`, name: `Feedback`, iconUrl: null },
    ]
    mocks.agent = { id: `a2`, role: `agent`, ownerId: `u1` }
    mocks.row = sessionRow({ agentId: `a2` })
    const field = renderComposer()
    type(field, `Hi`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(mocks.send.mock.calls[0]![0]).not.toHaveProperty(`scopeTeamIds`)
  })
})

// EXP-1537 (fixtures/composer-send.json): the field owns its text while a
// send is in flight; a success clears only what went out.
describe(`SessionComposer send in flight (EXP-1537)`, () => {
  function deferredSend() {
    let resolve!: (value: { txid: number; sessionId: string }) => void
    let reject!: (error: Error) => void
    mocks.send.mockImplementation(
      () =>
        new Promise((res, rej) => {
          resolve = res
          reject = rej
        })
    )
    return { resolve: (value = { txid: 1, sessionId: `s1` }) => resolve(value), reject: (error: Error) => reject(error) }
  }

  // #FEED-237 (message-delivery.json): a session composer clears at the
  // press; the message is an outbox bubble from then on.
  it(`the field clears at the press and the send carries a clientMessageId`, async () => {
    const pending = deferredSend()
    const field = renderComposer()
    type(field, `Ship it`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    expect(field.value).toBe(``)
    expect(mocks.send.mock.calls[0]![0]).toMatchObject({ text: `Ship it` })
    expect(typeof (mocks.send.mock.calls[0]![0] as { clientMessageId?: unknown }).clientMessageId).toBe(`string`)
    type(field, `and then the docs`)
    await act(async () => pending.resolve())
    expect(field.value).toBe(`and then the docs`)
  })

  it(`a failed send raises no toast and leaves the field alone`, async () => {
    const pending = deferredSend()
    const field = renderComposer()
    type(field, `Ship it`)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(1))
    type(field, `next`)
    await act(async () => pending.reject(new Error(`down`)))
    expect(mocks.toastError).not.toHaveBeenCalled()
    expect(field.value).toBe(`next`)
  })
})

// Radix opens a dropdown on the trigger's pointer DOWN.
function pointerOpen(element: HTMLElement) {
  act(() => {
    fireEvent.pointerDown(element, { button: 0, ctrlKey: false, pointerType: `mouse` })
  })
}

describe(`SessionComposer, the one composer (EXP-1334)`, () => {
  const openPlus = () => pointerOpen(screen.getByTestId(`agent-composer-plus-button`))
  const rowIds = () =>
    [...screen.getByTestId(`agent-composer-menu`).querySelectorAll(`[data-testid^="agent-composer-menu-"]`)].map(
      (row) => row.getAttribute(`data-testid`)!.replace(`agent-composer-menu-`, ``)
    )
  const pickModel = (label: string) => {
    act(() => {
      fireEvent.click(screen.getByTestId(`agent-composer-menu-model`))
    })
    act(() => {
      fireEvent.click(screen.getByText(label))
    })
  }

  // S-3 (composer-menu.json surface `session`): Photo or file, Issue ›, Model ›.
  it(`the "+" holds the session rows: file, issue, model; never the file chooser by itself`, () => {
    const click = vi.spyOn(HTMLInputElement.prototype, `click`)
    renderComposer()
    openPlus()
    expect(click).not.toHaveBeenCalled()
    expect(rowIds()).toEqual([`add-file`, `implement-issue`, `model`])
    click.mockRestore()
  })

  // #EXP-1535: Account › while the device is online and the viewer steers
  // the session; a pick = THE session account switch.
  it(`Account › offers the session's logins and a pick runs the account switch`, () => {
    mocks.accountSwitch = {
      canSteer: true,
      deviceOnline: true,
      blocked: null,
      switching: false,
      options: [
        { key: `claude:a`, agent: `claude`, email: `a@example.com` },
        { key: `claude:b`, agent: `claude`, email: `b@example.com` },
      ],
      value: `claude:a`,
      switchTo: mocks.switchTo,
    }
    try {
      renderComposer()
      openPlus()
      expect(rowIds()).toEqual([`add-file`, `implement-issue`, `account`, `model`])
      expect(screen.getByTestId(`agent-composer-menu-account`).textContent).toContain(`a@example.com`)
      act(() => {
        fireEvent.click(screen.getByTestId(`agent-composer-menu-account`))
      })
      act(() => {
        fireEvent.click(screen.getByText(`b@example.com`))
      })
      expect(mocks.switchTo).toHaveBeenCalledWith(`claude:b`)
    } finally {
      mocks.accountSwitch = null
    }
  })

  // #EXP-1434: Prompts › while the team has one; a pick inserts the body
  // at the caret (the `/` menu's pick, without a `/name` to replace).
  it(`Prompts › inserts the picked prompt's body at the caret`, async () => {
    mocks.prompts = [{ id: `p1`, name: `Release train`, icon: null, sortOrder: 1 }]
    mocks.promptGet.mockResolvedValue({ prompt: { id: `p1`, body: `Cut the release.` } })
    const field = renderComposer()
    type(field, `Then: `)
    openPlus()
    expect(rowIds()).toEqual([`add-file`, `implement-issue`, `prompts`, `model`])
    act(() => {
      fireEvent.click(screen.getByTestId(`agent-composer-menu-prompts`))
    })
    act(() => {
      fireEvent.click(
        within(screen.getByTestId(`agent-composer-prompts-picker`)).getByText(`Release train`)
      )
    })
    expect(screen.queryByTestId(`agent-composer-menu`)).toBeNull()
    await waitFor(() => expect(field.value).toBe(`Then: Cut the release.`))
    expect(mocks.promptGet).toHaveBeenCalledWith({ id: `p1` })
    expect(mocks.send).not.toHaveBeenCalled()
  })

  it(`the "+" is the only tool: no inline tools, no device or model pin in the card`, () => {
    renderComposer()
    for (const name of [`Add image`, `Attach files`, `Insert issue reference`, `Insert emoji`]) {
      expect(screen.queryByRole(`button`, { name })).toBeNull()
    }
    expect(screen.queryByTestId(`session-composer-options`)).toBeNull()
    expect(screen.queryByTestId(`session-composer-model`)).toBeNull()
    expect(document.querySelector(`[data-slot="composer-options"]`)).toBeNull()
  })

  // EXP-1401: the pick is the row's (sessions.configure), never a `/model`
  // turn a walled session would never take.
  it(`a model pick configures the session and reads as picked at once`, async () => {
    mocks.configure.mockResolvedValue({ txid: 1 })
    renderComposer()
    openPlus()
    expect(screen.getByTestId(`agent-composer-menu-model`).textContent).toContain(`Opus`)
    pickModel(`Sonnet`)
    await waitFor(() => expect(mocks.configure).toHaveBeenCalledTimes(1))
    expect(mocks.configure.mock.calls[0]![0]).toEqual({ sessionId: `s1`, model: `sonnet` })
    expect(mocks.send).not.toHaveBeenCalled()
    openPlus()
    expect(screen.getByTestId(`agent-composer-menu-model`).textContent).toContain(`Sonnet`)
  })

  it(`a refused model pick toasts Couldn't switch the model and reverts`, async () => {
    mocks.configure.mockRejectedValue(new Error(`This session is blocked by the haiku usage limit`))
    renderComposer()
    openPlus()
    pickModel(`Haiku`)
    await waitFor(() => expect(mocks.toastActionError).toHaveBeenCalledTimes(1))
    expect(mocks.toastActionError.mock.calls[0]![0]).toBe(`Couldn't switch the model`)
    openPlus()
    expect(screen.getByTestId(`agent-composer-menu-model`).textContent).toContain(`Opus`)
  })

  it(`a second model pick while one is in flight sends nothing`, async () => {
    mocks.configure.mockResolvedValue({ txid: 1 })
    renderComposer()
    openPlus()
    pickModel(`Sonnet`)
    openPlus()
    pickModel(`Haiku`)
    await waitFor(() => expect(mocks.configure).toHaveBeenCalledTimes(1))
    expect(mocks.configure.mock.calls[0]![0]).toEqual({ sessionId: `s1`, model: `sonnet` })
  })

  it(`an ended session cannot change its model`, () => {
    mocks.row = sessionRow({ status: `ended` })
    renderComposer()
    openPlus()
    expect(screen.getByTestId(`agent-composer-menu-model`).hasAttribute(`data-disabled`)).toBe(true)
  })

  it(`an agent chat with no conversation yet opens it with sessions.send({agentId})`, async () => {
    mocks.send.mockResolvedValue({ sessionId: `s-new`, txid: 3 })
    const onSent = vi.fn()
    render(
      <SessionComposer teamId="t1" sessionId={null} agentId="ag-1" surface="agent" onSent={onSent} placeholder="Message Triage" />
    )
    const field = screen.getByPlaceholderText(`Message Triage`) as HTMLTextAreaElement
    type(field, `hello`)
    pressEnter(field)
    await waitFor(() => expect(onSent).toHaveBeenCalledWith(`s-new`))
    expect(mocks.send.mock.calls[0]![0]).toMatchObject({ teamId: `t1`, agentId: `ag-1`, text: `hello` })
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
    const pills = screen.getAllByTestId(`chat-reply`)
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
    expect(screen.queryByTestId(`chat-replies`)).toBeNull()
  })
})

// #EXP-1793 (composer-menu.json `mcpPick`): an agent chat's MCP servers ›,
// seeded from the conversation's pick (`sessions.mcpPicks`), else the
// agent's `mcpServerIds`; only a changed pick rides the next send.
describe(`SessionComposer, an agent chat's MCP servers (#EXP-1793)`, () => {
  const SERVERS = [
    { id: `m1`, name: `GlitchTip`, url: `https://glitchtip.example/mcp`, command: null, connection: { status: `not_needed` } },
    { id: `m2`, name: `Linear`, url: `https://mcp.linear.app/mcp`, command: null, connection: { status: `not_needed` } },
  ]
  const openMcp = () => {
    pointerOpen(screen.getByTestId(`agent-composer-plus-button`))
    act(() => {
      fireEvent.click(screen.getByTestId(`agent-composer-menu-mcp-servers`))
    })
  }
  const send = async (field: HTMLTextAreaElement, text: string, calls: number) => {
    type(field, text)
    pressEnter(field)
    await waitFor(() => expect(mocks.send).toHaveBeenCalledTimes(calls))
    return mocks.send.mock.calls[calls - 1]![0] as Record<string, unknown>
  }

  it(`seeds from the conversation, sends a changed pick once`, async () => {
    mocks.mcpServers = SERVERS
    mocks.row = sessionRow({ agentId: `ag-1` })
    mocks.mcpPicks.mockResolvedValue({ mcpServerIds: [`m1`] })
    mocks.send.mockResolvedValue({ sessionId: `s1`, txid: 1 })
    render(<SessionComposer teamId="t1" sessionId="s1" surface="agent" placeholder="Message Triage" />)
    const field = screen.getByPlaceholderText(`Message Triage`) as HTMLTextAreaElement
    await waitFor(() => expect(mocks.mcpPicks).toHaveBeenCalledWith({ sessionId: `s1` }, expect.anything()))

    // Unchanged: the plain message.
    expect(await send(field, `one`, 1)).not.toHaveProperty(`mcpServerIds`)

    pointerOpen(screen.getByTestId(`agent-composer-plus-button`))
    await waitFor(() => expect(screen.getByTestId(`agent-composer-menu-mcp-servers`).textContent).toContain(`1`))
    act(() => {
      fireEvent.click(screen.getByTestId(`agent-composer-menu-mcp-servers`))
    })
    act(() => {
      fireEvent.click(screen.getByText(`Linear`))
    })
    expect(await send(field, `two`, 2)).toMatchObject({ sessionId: `s1`, mcpServerIds: [`m1`, `m2`] })
    // Sent: the new seed, nothing rides the next one.
    expect(await send(field, `three`, 3)).not.toHaveProperty(`mcpServerIds`)
  })

  it(`no conversation yet: seeds from the agent, an emptied pick sends []`, async () => {
    mocks.mcpServers = SERVERS
    mocks.row = null
    mocks.agent = { id: `ag-1`, role: `agent`, ownerId: `u2`, mcpServerIds: [`m2`] }
    mocks.send.mockResolvedValue({ sessionId: `s-new`, txid: 3 })
    render(
      <SessionComposer teamId="t1" sessionId={null} agentId="ag-1" surface="agent" onSent={() => {}} placeholder="Message Triage" />
    )
    const field = screen.getByPlaceholderText(`Message Triage`) as HTMLTextAreaElement
    expect(mocks.mcpPicks).not.toHaveBeenCalled()
    openMcp()
    act(() => {
      fireEvent.click(screen.getByText(`Linear`))
    })
    expect(await send(field, `hello`, 1)).toMatchObject({ agentId: `ag-1`, mcpServerIds: [] })
  })

  it(`a session's composer never asks for the pick`, () => {
    mocks.mcpServers = SERVERS
    renderComposer()
    pointerOpen(screen.getByTestId(`agent-composer-plus-button`))
    expect(screen.queryByTestId(`agent-composer-menu-mcp-servers`)).toBeNull()
    expect(mocks.mcpPicks).not.toHaveBeenCalled()
  })
})

// #EXP-1841 (composer-attachments.json): a chat's composer stages images
// exactly like the launch composer: `[Image #N]` at the caret, the X takes
// its placeholder along and renumbers the rest.
describe(`SessionComposer attachments (#EXP-1841)`, () => {
  URL.createObjectURL ??= () => ``
  URL.revokeObjectURL ??= () => undefined
  let urls = 0
  beforeEach(() => {
    urls = 0
    URL.createObjectURL = () => `blob:${++urls}`
  })
  const png = (name: string) => new File([new Uint8Array([1, 2, 3])], name, { type: `image/png` })
  function paste(field: HTMLTextAreaElement, files: File[]) {
    act(() => {
      fireEvent.paste(field, { clipboardData: { files, types: [`Files`] } })
    })
  }

  it(`a pasted image drops [Image #1] at the caret`, () => {
    const field = renderComposer()
    type(field, `please check spoken again`)
    field.setSelectionRange(0, 0)
    paste(field, [png(`a.png`)])
    expect(field.value).toBe(`[Image #1] please check spoken again`)
    expect(screen.getAllByRole(`button`, { name: `Remove image` })).toHaveLength(1)
  })

  it(`several images number on, and removing one renumbers the rest`, () => {
    const field = renderComposer()
    paste(field, [png(`a.png`), png(`b.png`)])
    expect(field.value).toBe(`[Image #1] [Image #2]`)
    act(() => {
      fireEvent.click(screen.getAllByRole(`button`, { name: `Remove image` })[0]!)
    })
    expect(field.value).toBe(`[Image #1]`)
    expect(screen.getAllByRole(`button`, { name: `Remove image` })).toHaveLength(1)
  })

  it(`the X keeps the caret where it was, minus what left before it`, async () => {
    const field = renderComposer()
    type(field, `crop  here`)
    field.setSelectionRange(5, 5)
    paste(field, [png(`a.png`)])
    expect(field.value).toBe(`crop [Image #1] here`)
    field.setSelectionRange(20, 20)
    act(() => {
      fireEvent.click(screen.getByRole(`button`, { name: `Remove image` }))
    })
    expect(field.value).toBe(`crop here`)
    await waitFor(() => expect(field.selectionStart).toBe(9))
  })

  it(`a fifth image toasts the cap and stays out`, () => {
    const field = renderComposer()
    paste(field, [png(`1.png`), png(`2.png`), png(`3.png`), png(`4.png`), png(`5.png`)])
    expect(screen.getAllByRole(`button`, { name: `Remove image` })).toHaveLength(4)
    expect(mocks.toastError).toHaveBeenCalledWith(`Up to 4 images per message`)
  })
})

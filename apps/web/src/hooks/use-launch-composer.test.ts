import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerDevice } from "@/lib/steer-devices"

// EXP-825 / EXP-1293: the composer's model: swap rules, submit labels, the
// blocked matrix, seed consumption and THE start, ONE `sessions.send` that
// opens the new thread. The synced collections are stubbed by ALIAS: the
// hook names its query sources (`p` prompts, `issues`), so the fake
// `useLiveQuery` hands each one its own rows.

const mockState = vi.hoisted(() => ({
  rows: {} as Record<string, unknown[]>,
  boards: [] as unknown[],
  repos: null as { id: string; fullName: string }[] | null,
  upload: vi.fn(),
  send: vi.fn(),
  navigate: vi.fn(),
  awaitTxId: vi.fn(),
  toastError: vi.fn(),
  // NEW-3: the team's agents (`useAgents`), the Message an agent › rows.
  agents: [] as unknown[],
}))

vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let alias = ``
      const chain = { where: () => chain, select: () => chain }
      const query = {
        from: (source: Record<string, unknown>) => {
          alias = Object.keys(source)[0] ?? ``
          return chain
        },
      }
      const result = build(query)
      if (result === undefined) return { data: undefined }
      return { data: mockState.rows[alias] ?? [] }
    },
  }
})
vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => mockState.navigate }))
vi.mock(`@/lib/collections`, () => ({
  promptCollection: {},
  issueCollection: {},
  sessionCollection: { utils: { awaitTxId: mockState.awaitTxId } },
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => mockState.boards,
  useTeamById: () => ({ id: `t1`, slug: `acme` }),
}))
vi.mock(`@/hooks/use-team-repos`, () => ({
  useTeamRepos: () => mockState.repos,
}))
vi.mock(`@/hooks/use-mcp-servers`, () => ({
  useMcpServers: () => ({ servers: null, error: null, refresh: vi.fn() }),
}))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useAgents: () => ({ agents: mockState.agents, isReady: true }),
}))
vi.mock(`@/lib/storage/issue-image-upload`, () => ({
  uploadTeamSessionImageFile: mockState.upload,
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { sessions: { send: { mutate: mockState.send } } },
}))
vi.mock(`@/components/agent-login-dialog`, () => ({ requestAgentLogin: vi.fn() }))
vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { error: mockState.toastError, dismiss: vi.fn() },
}))

import {
  buildSendInput,
  submitLabelFor,
  useLaunchComposer,
  type LaunchSeed,
} from "@/hooks/use-launch-composer"

const device: SteerDevice = {
  rowId: `row-1`,
  deviceId: `dev-1`,
  deviceLabel: `buildbox`,
  agents: [`claude`],
  caps: [`steer-files`],
  online: true,
  launchDefaults: {
    defaultAgent: `claude`,
    agents: { claude: {} },
  },
}

const board = (id: string, repositoryId: string | null, defaultBranch: string | null = null) => ({
  id,
  teamId: `t1`,
  name: id,
  repositoryId,
  defaultBranch,
})

const issue = (id: string, boardId: string, status = `backlog`) => ({
  id,
  boardId,
  teamId: `t1`,
  identifier: id.toUpperCase(),
  title: `Issue ${id}`,
  status,
  priority: `none`,
  createdAt: new Date(`2026-09-01T00:00:00Z`),
})

const prompt = (id: string, extra: Record<string, unknown> = {}) => ({
  id,
  teamId: `t1`,
  name: `Prompt ${id}`,
  icon: null,
  sortOrder: 1,
  createdAt: new Date(0),
  updatedAt: new Date(0),
  ...extra,
})

const agentRow = (agentId: string, name = `Triage`) => ({
  agentId,
  teamId: `t1`,
  name,
  icon: `bot`,
  color: `#8b5cf6`,
  busy: false,
  needsYou: false,
})

function mount(seed: LaunchSeed | null = null, devices: SteerDevice[] | null = [device]) {
  const onSeedConsumed = vi.fn()
  const onStarted = vi.fn()
  const hook = renderHook(() =>
    useLaunchComposer({ teamId: `t1`, devices, seed, onSeedConsumed, onStarted })
  )
  return { ...hook, onSeedConsumed, onStarted }
}

beforeEach(() => {
  mockState.rows = {
    p: [prompt(`pr-1`)],
    issues: [issue(`i1`, `b1`), issue(`i2`, `b1`), issue(`i3`, `b2`)],
  }
  mockState.boards = [board(`b1`, `repo-1`), board(`b2`, `repo-1`)]
  mockState.repos = [{ id: `repo-1`, fullName: `acme/app` }]
  mockState.agents = [agentRow(`ag-1`)]
  mockState.upload.mockReset()
  mockState.send.mockReset()
  mockState.send.mockResolvedValue({ sessionId: `s-new`, messageId: null, txid: 42 })
  mockState.navigate.mockReset()
  mockState.awaitTxId.mockReset()
  mockState.awaitTxId.mockResolvedValue(true)
  mockState.toastError.mockReset()
  vi.stubGlobal(`URL`, {
    ...URL,
    createObjectURL: vi.fn((file: File) => `blob:${file.name}`),
    revokeObjectURL: vi.fn(),
  })
})

describe(`submitLabelFor`, () => {
  it(`labels each subject`, () => {
    expect(submitLabelFor(null)).toBe(`Start session`)
    expect(submitLabelFor({ kind: `issues`, ids: [`a`] })).toBe(`Start coding`)
    expect(submitLabelFor({ kind: `issues`, ids: [`a`, `b`] })).toBe(`Start batch · 2`)
    expect(submitLabelFor({ kind: `prompt`, id: `x` })).toBe(`Start session`)
    // NEW-3: a DM sends; it starts nothing.
    expect(submitLabelFor({ kind: `agent`, id: `ag-1` })).toBe(`Send`)
  })
})

describe(`buildSendInput`, () => {
  const options = {
    agent: `claude`,
    model: `opus`,
    effort: ``,
    ultracode: false,
    account: `work`,
    mcpServerIds: [`m1`],
  }

  it(`maps the runner picks and an issue subject`, () => {
    expect(
      buildSendInput({
        teamId: `t1`,
        deviceId: `dev-1`,
        options,
        text: `  `,
        attachmentIds: [],
        subject: { kind: `issues`, ids: [`i1`, `i2`] },
        repoId: `repo-1`,
      })
    ).toEqual({
      teamId: `t1`,
      deviceId: `dev-1`,
      cli: `claude`,
      account: `work`,
      model: `opus`,
      ultracode: false,
      mcpServerIds: [`m1`],
      issueIds: [`i1`, `i2`],
    })
  })

  it(`sends a prompt beside its repository, a chat with its repository`, () => {
    const base = {
      teamId: `t1`,
      deviceId: `dev-1`,
      options: { ...options, account: undefined, mcpServerIds: undefined },
      text: `hello`,
      attachmentIds: [`att-1`],
      repoId: `repo-1`,
    }
    expect(buildSendInput({ ...base, subject: { kind: `prompt`, id: `pr-1` } })).toMatchObject({
      promptId: `pr-1`,
      repositoryId: `repo-1`,
      text: `hello`,
      attachmentIds: [`att-1`],
    })
    expect(
      buildSendInput({ ...base, subject: { kind: `prompt`, id: `pr-1` }, repoId: `` })
    ).not.toHaveProperty(`repositoryId`)
    const chat = buildSendInput({ ...base, subject: null })
    expect(chat).toMatchObject({ repositoryId: `repo-1` })
    expect(chat).not.toHaveProperty(`issueIds`)
    expect(chat).not.toHaveProperty(`promptId`)
    expect(buildSendInput({ ...base, subject: null, repoId: `` })).not.toHaveProperty(`repositoryId`)
  })
  it(`NEW-3: an agent subject is a DM: agentId, text and attachments, no runner picks`, () => {
    expect(
      buildSendInput({
        teamId: `t1`,
        deviceId: `dev-1`,
        options,
        text: `triage the inbox`,
        attachmentIds: [`att-1`],
        subject: { kind: `agent`, id: `ag-1` },
        repoId: `repo-1`,
      })
    ).toEqual({
      teamId: `t1`,
      agentId: `ag-1`,
      text: `triage the inbox`,
      attachmentIds: [`att-1`],
    })
  })
})

describe(`useLaunchComposer subject`, () => {
  it(`swaps between issues and a prompt`, () => {
    const { result } = mount()
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.toggleIssue(`i2`))
    expect(result.current.subject).toEqual({ kind: `issues`, ids: [`i1`, `i2`] })
    expect(result.current.submitLabel).toBe(`Start batch · 2`)
    act(() => result.current.pickPrompt(`pr-1`))
    expect(result.current.subject).toEqual({ kind: `prompt`, id: `pr-1` })
    expect(result.current.selectedPrompt?.name).toBe(`Prompt pr-1`)
    act(() => result.current.toggleIssue(`i3`))
    expect(result.current.subject).toEqual({ kind: `issues`, ids: [`i3`] })
    act(() => result.current.toggleIssue(`i3`))
    expect(result.current.subject).toBeNull()
    act(() => result.current.pickPrompt(`pr-1`))
    act(() => result.current.clearPrompt())
    expect(result.current.subject).toBeNull()
  })

  it(`lists the team's prompts in server order`, () => {
    mockState.rows.p = [prompt(`b`, { sortOrder: 2 }), prompt(`a`, { sortOrder: 1 })]
    const { result } = mount()
    expect(result.current.prompts.map((row) => row.id)).toEqual([`a`, `b`])
  })

  it(`blocks a subjectless start without text; a synced prompt needs none`, () => {
    const { result } = mount()
    expect(result.current.blocked).toBe(true)
    act(() => result.current.setText(`hi`))
    expect(result.current.blocked).toBe(false)
    act(() => result.current.setText(``))
    act(() => result.current.pickPrompt(`pr-1`))
    expect(result.current.blocked).toBe(false)
    // A prompt whose row is not synced (or gone) cannot start.
    act(() => result.current.pickPrompt(`pr-gone`))
    expect(result.current.blocked).toBe(true)
  })

  it(`blocks a batch spanning repositories`, () => {
    mockState.boards = [board(`b1`, `repo-1`), board(`b2`, `repo-2`)]
    const { result } = mount()
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.toggleIssue(`i3`))
    expect(result.current.spansRepos).toBe(true)
    expect(result.current.blocked).toBe(true)
  })

  it(`blocks without a device`, () => {
    const { result } = mount(null, [])
    act(() => result.current.setText(`hi`))
    expect(result.current.blocked).toBe(true)
  })

  it(`NEW-3: an agent is a subject like the others: it swaps, and a DM needs no device`, () => {
    const { result } = mount(null, [])
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.pickAgent(`ag-1`))
    expect(result.current.subject).toEqual({ kind: `agent`, id: `ag-1` })
    expect(result.current.selectedAgent?.name).toBe(`Triage`)
    expect(result.current.submitLabel).toBe(`Send`)
    // Something to say, then the DM sends without any machine of the caller's.
    expect(result.current.blocked).toBe(true)
    act(() => result.current.setText(`hi`))
    expect(result.current.blocked).toBe(false)
    // An agent whose row is not synced (or gone) cannot be messaged.
    act(() => result.current.pickAgent(`ag-gone`))
    expect(result.current.blocked).toBe(true)
    act(() => result.current.toggleIssue(`i2`))
    expect(result.current.subject).toEqual({ kind: `issues`, ids: [`i2`] })
    act(() => result.current.pickAgent(`ag-1`))
    act(() => result.current.clearAgent())
    expect(result.current.subject).toBeNull()
  })

  it(`consumes a seed once: issues, text only into an empty draft`, () => {
    const { result, onSeedConsumed } = mount({ issueIds: [`i1`, `i1`], text: `do it` })
    expect(result.current.subject).toEqual({ kind: `issues`, ids: [`i1`] })
    expect(result.current.text).toBe(`do it`)
    expect(onSeedConsumed).toHaveBeenCalledTimes(1)
  })
})

describe(`useLaunchComposer start`, () => {
  it(`sends ONE sessions.send in the team and opens the new thread`, async () => {
    const { result, onStarted } = mount()
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.setText(`keep it small`))
    await act(() => result.current.submit())
    expect(mockState.send).toHaveBeenCalledTimes(1)
    expect(mockState.send.mock.calls[0]![0]).toMatchObject({
      teamId: `t1`,
      deviceId: `dev-1`,
      cli: `claude`,
      issueIds: [`i1`],
      text: `keep it small`,
    })
    expect(mockState.awaitTxId).toHaveBeenCalledWith(42)
    expect(onStarted).toHaveBeenCalledWith(`s-new`)
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `s-new` },
    })
    // The draft is done with.
    expect(result.current.subject).toBeNull()
    expect(result.current.text).toBe(``)
  })

  it(`NEW-3: a DM sends sessions.send({agentId}) and opens the agent's conversation`, async () => {
    const { result, onStarted } = mount(null, [])
    act(() => result.current.pickAgent(`ag-1`))
    act(() => result.current.setText(`what is open?`))
    await act(() => result.current.submit())
    expect(mockState.send).toHaveBeenCalledTimes(1)
    expect(mockState.send.mock.calls[0]![0]).toEqual({
      teamId: `t1`,
      agentId: `ag-1`,
      text: `what is open?`,
    })
    expect(onStarted).toHaveBeenCalledWith(`s-new`)
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug: `acme`, agentId: `ag-1` },
    })
    expect(result.current.subject).toBeNull()
    expect(result.current.text).toBe(``)
  })

  it(`starts a prompt run: promptId beside the repository and the typed text`, async () => {
    const { result } = mount()
    act(() => result.current.pickPrompt(`pr-1`))
    act(() => result.current.setText(`only the web app`))
    await act(() => result.current.submit())
    expect(mockState.send.mock.calls[0]![0]).toMatchObject({
      teamId: `t1`,
      promptId: `pr-1`,
      repositoryId: `repo-1`,
      text: `only the web app`,
    })
    expect(result.current.subject).toBeNull()
  })

  it(`uploads files first and sends their ids`, async () => {
    mockState.upload.mockResolvedValue({ id: `att-1`, filename: `shot.png` })
    const { result } = mount()
    act(() => {
      result.current.addFiles([new File([`x`], `shot.png`, { type: `image/png` })], 0)
    })
    await act(() => result.current.submit())
    expect(mockState.upload).toHaveBeenCalledTimes(1)
    expect(mockState.send.mock.calls[0]![0]).toMatchObject({
      attachmentIds: [`att-1`],
      repositoryId: `repo-1`,
    })
  })

  it(`keeps the draft and toasts when the send fails`, async () => {
    mockState.send.mockRejectedValue(new Error(`nope`))
    const { result, onStarted } = mount()
    act(() => result.current.setText(`hi`))
    await act(() => result.current.submit())
    expect(mockState.toastError).toHaveBeenCalled()
    expect(onStarted).not.toHaveBeenCalled()
    expect(mockState.navigate).not.toHaveBeenCalled()
    expect(result.current.text).toBe(`hi`)
    expect(result.current.busy).toBe(false)
  })

  it(`opens the thread anyway when the sync wait gives up`, async () => {
    vi.useFakeTimers()
    try {
      mockState.awaitTxId.mockReturnValue(new Promise(() => {}))
      const { result } = mount()
      act(() => result.current.setText(`hi`))
      let done: Promise<void> | undefined
      act(() => {
        done = result.current.submit()
      })
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5_000)
        await done
      })
      expect(mockState.navigate).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })
})

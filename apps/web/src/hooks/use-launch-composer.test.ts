import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerDevice } from "@/lib/steer-devices"
import {
  BUILTIN_CHAT_ID,
  BUILTIN_CREATE_ACTION_ID,
  BUILTIN_FIX_CONFLICTS_ID,
  BUILTIN_TIDY_UP_ID,
} from "@/lib/builtin-actions"

// EXP-825: the composer's model — swap rules, submit labels, the blocked
// matrix, image markers, prompt composition and seed consumption. The synced
// collections are stubbed by ALIAS: the hook names its query sources `a`
// (actions), `issues`, `s` (sessions) and `w` (worktrees), so the fake
// `useLiveQuery` hands each one its own rows.

const mockState = vi.hoisted(() => ({
  rows: {
    a: [] as unknown[],
    issues: [] as unknown[],
    s: [] as unknown[],
    w: [] as unknown[],
    r: [] as unknown[],
    bl: [] as unknown[],
    tr: [] as unknown[],
    li: [] as unknown[],
  } as Record<string, unknown[]>,
  boards: [] as unknown[],
  repos: null as { id: string; fullName: string }[] | null,
  mcpServers: null as unknown[] | null,
  upload: vi.fn(),
  toastError: vi.fn(),
  toastDismiss: vi.fn(),
}))

vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let alias = ``
      const chain = { where: () => chain }
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
vi.mock(`@/lib/collections`, () => ({
  actionCollection: {},
  codingSessionCollection: {},
  deviceWorktreeCollection: {},
  issueCollection: {},
  issueRelationCollection: {},
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => mockState.boards,
  useTeamById: () => ({ id: `t1`, slug: `acme` }),
}))
vi.mock(`@/hooks/use-team-repos`, () => ({
  useTeamRepos: () => mockState.repos,
}))
vi.mock(`@/hooks/use-mcp-servers`, () => ({
  useMcpServers: () => ({
    servers: mockState.mcpServers,
    error: null,
    refresh: vi.fn(),
  }),
}))
vi.mock(`@/lib/storage/issue-image-upload`, () => ({
  uploadTeamSessionImageFile: mockState.upload,
}))
vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { error: mockState.toastError, dismiss: mockState.toastDismiss },
}))

import {
  draftStartFingerprint,
  useLaunchComposer,
  type LaunchSeed,
} from "@/hooks/use-launch-composer"
import type { RemoteStart } from "@/hooks/use-remote-start"
import { stackedStartPrompt } from "@/lib/blocked-start"

const device: SteerDevice = {
  rowId: `row-1`,
  deviceId: `dev-1`,
  deviceLabel: `buildbox`,
  agents: [`claude`],
  // F6: a current build localizes file attachments.
  caps: [`steer-files`],
  online: true,
  launchDefaults: {
    defaultAgent: `claude`,
    agents: { claude: { planMode: true } },
  },
}

const board = (id: string, repositoryId: string, defaultBranch: string | null = null) => ({
  id,
  teamId: `t1`,
  name: id,
  repositoryId,
  defaultBranch,
})

const issue = (id: string, boardId: string, status = `backlog`) => ({
  id,
  boardId,
  identifier: id.toUpperCase(),
  title: `Issue ${id}`,
  status,
  priority: `none`,
  createdAt: new Date(`2026-09-01T00:00:00Z`),
})

const action = (id: string, inputs: unknown[] = []) => ({
  id,
  teamId: `t1`,
  repositoryId: null,
  name: `Action ${id}`,
  description: null,
  icon: null,
  inputs,
  sortOrder: 1,
  createdAt: new Date(0),
  updatedAt: new Date(0),
})

function makeRemote(overrides: Partial<RemoteStart> = {}): RemoteStart {
  return {
    devices: [device],
    starting: false,
    sentTo: null,
    startIssues: vi.fn().mockResolvedValue(undefined),
    runAction: vi.fn().mockResolvedValue(undefined),
    supersede: vi.fn(),
    quietWatch: vi.fn(),
    openedId: null,
    refresh: vi.fn(),
    latestVersions: null,
    ...overrides,
  }
}

function mount(seed: LaunchSeed | null = null, remote = makeRemote()) {
  const onSeedConsumed = vi.fn()
  const onLanded = vi.fn()
  const hook = renderHook(
    ({ seed: current, remote: currentRemote }: { seed: LaunchSeed | null; remote?: RemoteStart }) =>
      useLaunchComposer({
        teamId: `t1`,
        remote: currentRemote ?? remote,
        seed: current,
        onSeedConsumed,
        onLanded,
      }),
    { initialProps: { seed, remote } as { seed: LaunchSeed | null; remote?: RemoteStart } }
  )
  // EXP-1271: the remote watch opened the started run (a newer `openedId`):
  // the run landed.
  const land = (openedId = `run-1`) => {
    hook.rerender({ seed, remote: { ...remote, openedId } })
  }
  return { ...hook, remote, onSeedConsumed, onLanded, land }
}

beforeEach(() => {
  mockState.rows.a = [action(`act-1`)]
  mockState.rows.issues = [issue(`i1`, `b1`), issue(`i2`, `b1`), issue(`i3`, `b2`)]
  mockState.rows.s = []
  mockState.rows.w = []
  mockState.rows.r = []
  mockState.rows.bl = []
  mockState.rows.tr = []
  mockState.rows.li = []
  mockState.boards = [board(`b1`, `repo-1`), board(`b2`, `repo-1`)]
  mockState.repos = [{ id: `repo-1`, fullName: `acme/app` }]
  mockState.mcpServers = null
  mockState.upload.mockReset()
  mockState.toastError.mockReset()
  mockState.toastDismiss.mockReset()
  vi.stubGlobal(`URL`, {
    ...URL,
    createObjectURL: vi.fn((file: File) => `blob:${file.name}`),
    revokeObjectURL: vi.fn(),
  })
})

describe(`useLaunchComposer subject`, () => {
  it(`starts as a chat and labels each subject per the contract`, () => {
    const { result } = mount()
    expect(result.current.subject).toBeNull()
    expect(result.current.submitLabel).toBe(`Start chat`)

    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.submitLabel).toBe(`Start coding`)
    act(() => result.current.toggleIssue(`i2`))
    expect(result.current.submitLabel).toBe(`Start batch · 2`)

    act(() => result.current.pickAction(`act-1`))
    expect(result.current.submitLabel).toBe(`Run action`)
  })

  it(`swaps between issues and an action instead of disabling anything`, () => {
    const { result } = mount()
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.toggleIssue(`i2`))
    // Picking an action REPLACES the issue chips.
    act(() => result.current.pickAction(`act-1`))
    expect(result.current.subject).toEqual({
      kind: `action`,
      id: `act-1`,
      inputs: {},
    })
    // Picking an issue REPLACES the action chip.
    act(() => result.current.toggleIssue(`i3`))
    expect(result.current.subject).toEqual({ kind: `issues`, ids: [`i3`] })
    // Unchecking the last issue is a chat again.
    act(() => result.current.toggleIssue(`i3`))
    expect(result.current.subject).toBeNull()
    // The hidden Chat builtin is the no-subject state, never a chip.
    act(() => result.current.pickAction(`act-1`))
    act(() => result.current.pickAction(BUILTIN_CHAT_ID))
    expect(result.current.subject).toBeNull()
  })

  it(`lists fix-conflicts, Create action, Tidy up, then the team's rows`, () => {
    const { result } = mount()
    expect(result.current.actions?.map((a) => a.id)).toEqual([
      BUILTIN_FIX_CONFLICTS_ID,
      BUILTIN_CREATE_ACTION_ID,
      BUILTIN_TIDY_UP_ID,
      `act-1`,
    ])
  })

  it(`offers only codeable, not-running issues on repo-backed boards`, () => {
    mockState.boards = [board(`b1`, `repo-1`), { ...board(`b2`, ``), repositoryId: null }]
    mockState.rows.issues = [
      issue(`i1`, `b1`),
      issue(`done`, `b1`, `done`),
      issue(`busy`, `b1`, `in_progress`),
    ]
    mockState.rows.s = [
      { issueId: `busy`, status: `running`, teamId: `t1`, updatedAt: new Date() },
    ]
    const { result } = mount()
    expect(result.current.eligibleIssues.map((i) => i.id)).toEqual([`i1`])
  })
})

describe(`useLaunchComposer blocked matrix`, () => {
  it(`a chat needs text or an image`, () => {
    const { result } = mount()
    expect(result.current.blocked).toBe(true)
    act(() => result.current.setText(`hello`))
    expect(result.current.blocked).toBe(false)
    act(() => result.current.setText(`   `))
    expect(result.current.blocked).toBe(true)
    act(() => {
      result.current.addFiles([new File([`x`], `a.png`, { type: `image/png` })], 0)
    })
    expect(result.current.blocked).toBe(false)
  })

  it(`Create action needs the request text; a required pick gates an action`, () => {
    const { result } = mount()
    act(() => result.current.pickAction(BUILTIN_CREATE_ACTION_ID))
    expect(result.current.blocked).toBe(true)
    act(() => result.current.setText(`Nightly triage`))
    expect(result.current.blocked).toBe(false)

    act(() => result.current.pickAction(BUILTIN_FIX_CONFLICTS_ID))
    expect(result.current.blocked).toBe(true)
    act(() => result.current.setInput(`pr`, `i1`))
    expect(result.current.blocked).toBe(false)
  })

  it(`issues: the cap, one repository and one base branch`, () => {
    mockState.boards = [
      board(`b1`, `repo-1`),
      board(`b2`, `repo-2`),
      board(`b3`, `repo-1`, `release/1.x`),
      board(`b4`, `repo-1`, `main`),
    ]
    mockState.rows.issues = [
      issue(`i1`, `b1`),
      issue(`i2`, `b2`),
      issue(`i3`, `b3`),
      issue(`i4`, `b4`),
    ]
    const { result } = mount()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.blocked).toBe(false)
    // Text is OPTIONAL once there is a subject.
    expect(result.current.text).toBe(``)

    act(() => result.current.toggleIssue(`i2`))
    expect(result.current.spansRepos).toBe(true)
    expect(result.current.blocked).toBe(true)
    act(() => result.current.toggleIssue(`i2`))

    act(() => result.current.toggleIssue(`i3`))
    act(() => result.current.toggleIssue(`i4`))
    expect(result.current.spansBranches).toBe(true)
    expect(result.current.blocked).toBe(true)
  })

  it(`blocks without a device or with an agent the machine cannot drive`, () => {
    const { result: none } = mount(null, makeRemote({ devices: [] }))
    act(() => none.current.setText(`hi`))
    expect(none.current.blocked).toBe(true)

    const { result: notReady } = mount(
      null,
      makeRemote({ devices: [{ ...device, acpAgents: [`codex`] }] })
    )
    act(() => notReady.current.setText(`hi`))
    expect(notReady.current.launch.agentNotReady).toBe(true)
    expect(notReady.current.blocked).toBe(true)
  })
})

describe(`useLaunchComposer plan mode`, () => {
  it(`is off for a chat and reseeds from the device once a subject is picked`, () => {
    const { result } = mount()
    expect(result.current.launch.planMode).toBe(false)
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.launch.planMode).toBe(true)
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.launch.planMode).toBe(false)
  })
})

describe(`useLaunchComposer images`, () => {
  it(`drops a marker at the caret and renumbers on removal`, () => {
    const { result } = mount()
    act(() => result.current.setText(`crop`))
    let caret = 0
    act(() => {
      caret = result.current.addFiles(
        [
          new File([`x`], `a.png`, { type: `image/png` }),
          new File([`x`], `b.png`, { type: `image/png` }),
        ],
        4
      )
    })
    expect(result.current.text).toBe(`crop [Image #1] [Image #2]`)
    expect(caret).toBe(result.current.text.length)
    expect(result.current.images.map((image) => image.url)).toEqual([
      `blob:a.png`,
      `blob:b.png`,
    ])
    act(() => result.current.removeImage(`blob:a.png`))
    expect(result.current.text).toBe(`crop [Image #1]`)
    expect(result.current.images).toHaveLength(1)
  })
})

describe(`useLaunchComposer submit`, () => {
  it(`starts a chat on the hidden builtin with the text as the prompt`, async () => {
    const { result, remote, land, onLanded } = mount()
    act(() => result.current.setText(`  Explain #I1  `))
    await act(() => result.current.submit())
    expect(remote.runAction).toHaveBeenCalledWith(
      device,
      { id: BUILTIN_CHAT_ID, name: `Chat`, teamId: `t1` },
      expect.objectContaining({ agent: `claude`, planMode: false }),
      // The only repo pre-picks itself (EXP-822) and rides as the `repo` input.
      { repo: `repo-1` },
      `Explain #I1`,
      expect.any(Function),
      expect.any(String)
    )
    // EXP-1271: the draft stays while the run starts; it clears once the
    // run's row lands.
    expect(result.current.starting?.deviceLabel).toBe(`buildbox`)
    expect(result.current.busy).toBe(true)
    expect(result.current.text).toBe(`  Explain #I1  `)
    land()
    expect(result.current.starting).toBeNull()
    expect(result.current.text).toBe(``)
    expect(result.current.subject).toBeNull()
    expect(onLanded).toHaveBeenCalledTimes(1)
  })

  it(`starts issues with the text as additional instructions`, async () => {
    const { result, remote, land } = mount()
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.toggleIssue(`i2`))
    act(() => result.current.setText(`Keep the API stable`))
    await act(() => result.current.submit())
    expect(remote.startIssues).toHaveBeenCalledWith(
      device,
      expect.objectContaining({ agent: `claude`, planMode: true }),
      [`i1`, `i2`],
      `Keep the API stable`,
      expect.any(Function),
      expect.any(String)
    )
    land()
    expect(result.current.subject).toBeNull()
  })

  it(`omits an empty prompt on an action run and sends the picks`, async () => {
    const { result, remote } = mount()
    act(() => result.current.pickAction(BUILTIN_FIX_CONFLICTS_ID))
    act(() => result.current.setInput(`pr`, `i1`))
    await act(() => result.current.submit())
    expect(remote.runAction).toHaveBeenCalledWith(
      device,
      { id: BUILTIN_FIX_CONFLICTS_ID, name: `Fix merge conflicts`, teamId: `t1` },
      expect.objectContaining({ agent: `claude` }),
      { pr: `i1` },
      undefined,
      expect.any(Function),
      expect.any(String)
    )
  })

  it(`uploads images sequentially and embeds them in the prompt`, async () => {
    mockState.upload
      .mockResolvedValueOnce({ id: `att-1` })
      .mockResolvedValueOnce({ id: `att-2` })
    const { result, remote, land } = mount()
    act(() => result.current.setText(`see`))
    act(() => {
      result.current.addFiles(
        [
          new File([`x`], `a.png`, { type: `image/png` }),
          new File([`x`], `b.png`, { type: `image/png` }),
        ],
        3
      )
    })
    await act(() => result.current.submit())
    expect(mockState.upload).toHaveBeenCalledTimes(2)
    expect(mockState.upload).toHaveBeenNthCalledWith(1, `t1`, expect.any(File))
    expect(remote.runAction).toHaveBeenCalledWith(
      device,
      expect.anything(),
      expect.anything(),
      { repo: `repo-1` },
      `see [Image #1] [Image #2]\n\n![image](/api/attachments/att-1)\n![image](/api/attachments/att-2)`,
      expect.any(Function),
      expect.any(String)
    )
    land()
    expect(result.current.images).toEqual([])
  })

  it(`uploads files beside images and links them after the embeds`, async () => {
    mockState.upload
      .mockResolvedValueOnce({ id: `att-1`, filename: `a.png` })
      .mockResolvedValueOnce({ id: `att-2`, filename: `notes [v2].pdf` })
    const { result, remote } = mount()
    act(() => result.current.setText(`see`))
    act(() => {
      result.current.addFiles(
        [
          new File([`x`], `a.png`, { type: `image/png` }),
          new File([`x`], `notes [v2].pdf`, { type: `application/pdf` }),
        ],
        3
      )
    })
    expect(result.current.text).toBe(`see [Image #1]`)
    expect(result.current.images.map((entry) => entry.kind)).toEqual([
      `image`,
      `file`,
    ])
    await act(() => result.current.submit())
    expect(remote.runAction).toHaveBeenCalledWith(
      device,
      expect.anything(),
      expect.anything(),
      { repo: `repo-1` },
      `see [Image #1]\n\n![image](/api/attachments/att-1)\n[notes [v2\\].pdf](/api/attachments/att-2)`,
      expect.any(Function),
      expect.any(String)
    )
  })

  it(`toasts the one rejection copy and the file cap`, () => {
    const { result } = mount()
    const big = new File([`x`], `huge.zip`, { type: `application/zip` })
    Object.defineProperty(big, `size`, { value: 51 * 1024 * 1024 })
    act(() => {
      result.current.addFiles(
        [
          big,
          ...[1, 2, 3, 4, 5].map(
            (n) => new File([`x`], `${n}.txt`, { type: `text/plain` })
          ),
        ],
        0
      )
    })
    expect(mockState.toastError).toHaveBeenCalledWith(
      `Images up to 10 MB and files up to 50 MB can be attached`
    )
    expect(mockState.toastError).toHaveBeenCalledWith(`Up to 4 files per message`)
    expect(result.current.images).toHaveLength(4)
  })

  it(`keeps the draft on an upload failure and retries only the rest`, async () => {
    mockState.upload
      .mockResolvedValueOnce({ id: `att-1` })
      .mockRejectedValueOnce(new Error(`boom`))
      .mockResolvedValueOnce({ id: `att-2` })
    const { result, remote } = mount()
    act(() => {
      result.current.addFiles(
        [
          new File([`x`], `a.png`, { type: `image/png` }),
          new File([`x`], `b.png`, { type: `image/png` }),
        ],
        0
      )
    })
    await act(() => result.current.submit())
    expect(remote.runAction).not.toHaveBeenCalled()
    expect(mockState.toastError).toHaveBeenCalledWith(
      `Couldn't upload the attachment`,
      expect.objectContaining({ action: expect.objectContaining({ label: `Retry` }) })
    )
    expect(result.current.images[0]!.uploadedId).toBe(`att-1`)
    expect(result.current.images).toHaveLength(2)

    await act(() => result.current.submit())
    // The first image's id was persisted: only the second uploads again.
    expect(mockState.upload).toHaveBeenCalledTimes(3)
    expect(remote.runAction).toHaveBeenCalledTimes(1)
  })

  it(`keeps the draft when the start itself is refused`, async () => {
    const remote = makeRemote({
      runAction: vi.fn().mockRejectedValue(new Error(`refused`)),
    })
    const { result } = mount(null, remote)
    act(() => result.current.setText(`hi`))
    await act(() => result.current.submit())
    expect(result.current.text).toBe(`hi`)
    expect(result.current.starting).toBeNull()
  })

  // EXP-1271 + review #3: the deadline counts from the SENT request (after
  // the uploads); a timeout toasts with Retry, the composer goes idle, the
  // watch keeps looking quietly, the draft stays.
  it(`gives up 30 s after the request was sent and keeps the draft for Retry`, async () => {
    vi.useFakeTimers()
    try {
      const { result, remote } = mount()
      act(() => result.current.setText(`hi`))
      await act(() => result.current.submit())
      expect(result.current.starting).not.toBeNull()
      act(() => vi.advanceTimersByTime(29_999))
      expect(result.current.starting).not.toBeNull()
      act(() => vi.advanceTimersByTime(1))
      expect(remote.quietWatch).toHaveBeenCalled()
      expect(result.current.starting).toBeNull()
      expect(result.current.busy).toBe(false)
      expect(result.current.text).toBe(`hi`)
      expect(mockState.toastError).toHaveBeenCalledWith(`The session did not start`, {
        description: `buildbox did not answer.`,
        action: { label: `Retry`, onClick: expect.any(Function) },
      })
    } finally {
      vi.useRealTimers()
    }
  })

  it(`never counts the uploads against the deadline`, async () => {
    vi.useFakeTimers()
    try {
      let finishUpload: (value: { id: string }) => void = () => {}
      mockState.upload.mockImplementationOnce(
        () => new Promise((resolve) => (finishUpload = resolve))
      )
      const { result, remote } = mount()
      act(() => {
        result.current.addFiles([new File([`x`], `a.png`, { type: `image/png` })], 0)
      })
      let submitted: Promise<void> = Promise.resolve()
      act(() => {
        submitted = result.current.submit()
      })
      // A 40 s upload: no give-up toast while it runs.
      act(() => vi.advanceTimersByTime(40_000))
      expect(mockState.toastError).not.toHaveBeenCalled()
      expect(result.current.starting).not.toBeNull()
      await act(async () => {
        finishUpload({ id: `att-1` })
        await submitted
      })
      expect(remote.runAction).toHaveBeenCalledTimes(1)
      act(() => vi.advanceTimersByTime(29_000))
      expect(mockState.toastError).not.toHaveBeenCalled()
      act(() => vi.advanceTimersByTime(1_000))
      expect(mockState.toastError).toHaveBeenCalledWith(
        `The session did not start`,
        expect.anything()
      )
    } finally {
      vi.useRealTimers()
    }
  })

  // Final review B1/B4: ONE startId per draft start. Retry AND a later
  // Start press of the unchanged draft reuse it; an edited draft is a
  // fresh start.
  it(`reuses the startId for Retry and a later Start of the same draft`, async () => {
    vi.useFakeTimers()
    try {
      const { result, remote } = mount()
      act(() => result.current.setText(`hi`))
      await act(() => result.current.submit())
      const first = vi.mocked(remote.runAction).mock.calls[0]![6]
      expect(first).toMatch(/^[0-9a-f-]{36}$/)
      act(() => vi.advanceTimersByTime(30_000))
      const retry = mockState.toastError.mock.calls.at(-1)![1].action.onClick
      await act(async () => retry())
      expect(remote.runAction).toHaveBeenCalledTimes(2)
      expect(vi.mocked(remote.runAction).mock.calls[1]![6]).toBe(first)
      act(() => vi.advanceTimersByTime(30_000))
      // The obvious move after a timeout: Start again, same draft.
      await act(() => result.current.submit())
      expect(vi.mocked(remote.runAction).mock.calls[2]![6]).toBe(first)
      act(() => vi.advanceTimersByTime(30_000))
      // An edited draft is another start, Retry included.
      act(() => result.current.setText(`hi there`))
      const staleRetry = mockState.toastError.mock.calls.at(-1)![1].action.onClick
      await act(async () => staleRetry())
      const fourth = vi.mocked(remote.runAction).mock.calls[3]![6]
      expect(fourth).toMatch(/^[0-9a-f-]{36}$/)
      expect(fourth).not.toBe(first)
    } finally {
      vi.useRealTimers()
    }
  })

  it(`a landing ends the draft start: the next Start mints a fresh id`, async () => {
    const remote = makeRemote()
    const { result, rerender } = mount(null, remote)
    act(() => result.current.setText(`hi`))
    await act(() => result.current.submit())
    const first = vi.mocked(remote.runAction).mock.calls[0]![6]
    rerender({ seed: null, remote: { ...remote, openedId: `run-1` } })
    act(() => result.current.setText(`hi`))
    await act(() => result.current.submit())
    expect(vi.mocked(remote.runAction).mock.calls[1]![6]).not.toBe(first)
  })

  it(`a refused start's Retry reuses the startId too`, async () => {
    const runAction = vi.fn().mockRejectedValueOnce(new Error(`refused`))
    runAction.mockResolvedValue(undefined)
    const { result } = mount(null, makeRemote({ runAction }))
    act(() => result.current.setText(`hi`))
    await act(() => result.current.submit())
    const retry = runAction.mock.calls[0]![5] as () => void
    await act(async () => retry())
    expect(runAction).toHaveBeenCalledTimes(2)
    expect(runAction.mock.calls[1]![6]).toBe(runAction.mock.calls[0]![6])
  })

  it(`a row landing after the timeout dismisses the error, clears the draft and lands`, async () => {
    vi.useFakeTimers()
    try {
      mockState.toastError.mockReturnValue(`toast-7`)
      const { result, land, onLanded } = mount()
      act(() => result.current.setText(`hi`))
      await act(() => result.current.submit())
      act(() => vi.advanceTimersByTime(30_000))
      expect(result.current.text).toBe(`hi`)
      land()
      expect(mockState.toastDismiss).toHaveBeenCalledWith(`toast-7`)
      expect(result.current.text).toBe(``)
      expect(result.current.starting).toBeNull()
      expect(onLanded).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  // Final review B2: the person typed a new prompt after the timeout; the
  // first start's late row clears the error only, the field stays and the
  // watch is told not to navigate.
  it(`a late row after the draft changed keeps the field and does not land`, async () => {
    vi.useFakeTimers()
    try {
      mockState.toastError.mockReturnValue(`toast-8`)
      const { result, land, onLanded, remote } = mount()
      act(() => result.current.setText(`hi`))
      await act(() => result.current.submit())
      act(() => vi.advanceTimersByTime(30_000))
      const canOpen = vi.mocked(remote.quietWatch).mock.calls[0]![0]!
      expect(canOpen()).toBe(true)
      act(() => result.current.setText(`something else`))
      expect(canOpen()).toBe(false)
      land()
      expect(mockState.toastDismiss).toHaveBeenCalledWith(`toast-8`)
      expect(result.current.text).toBe(`something else`)
      expect(onLanded).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })

  // Final review B6: an EDITED draft started after the timeout supersedes
  // the first start's watch before its uploads: the first run's late row
  // never ends the new start nor touches its draft.
  it(`a different start supersedes the previous watch before its uploads`, async () => {
    vi.useFakeTimers()
    try {
      const { result, remote, land, onLanded } = mount()
      act(() => result.current.setText(`hi`))
      await act(() => result.current.submit())
      const first = vi.mocked(remote.runAction).mock.calls[0]![6]
      expect(remote.supersede).toHaveBeenLastCalledWith(first)
      act(() => vi.advanceTimersByTime(30_000))
      act(() => result.current.setText(`something else`))
      let finishUpload: (value: { id: string }) => void = () => {}
      mockState.upload.mockImplementationOnce(
        () => new Promise((resolve) => (finishUpload = resolve))
      )
      act(() => {
        result.current.addFiles([new File([`x`], `a.png`, { type: `image/png` })], 0)
      })
      const draft = result.current.text
      let submitted: Promise<void> = Promise.resolve()
      act(() => {
        submitted = result.current.submit()
      })
      const second = vi.mocked(remote.supersede).mock.calls.at(-1)![0]
      expect(second).toMatch(/^[0-9a-f-]{36}$/)
      expect(second).not.toBe(first)
      expect(remote.runAction).toHaveBeenCalledTimes(1)
      // The first start's row lands while the new start uploads.
      land()
      expect(result.current.text).toBe(draft)
      expect(result.current.starting).not.toBeNull()
      expect(onLanded).not.toHaveBeenCalled()
      await act(async () => {
        finishUpload({ id: `att-1` })
        await submitted
      })
      expect(remote.runAction).toHaveBeenCalledTimes(2)
      expect(vi.mocked(remote.runAction).mock.calls[1]![6]).toBe(second)
    } finally {
      vi.useRealTimers()
    }
  })

  it(`a re-send of the same draft keeps the watch: the original row lands it`, async () => {
    vi.useFakeTimers()
    try {
      const { result, remote, land, onLanded } = mount()
      act(() => result.current.setText(`hi`))
      await act(() => result.current.submit())
      const first = vi.mocked(remote.runAction).mock.calls[0]![6]
      act(() => vi.advanceTimersByTime(30_000))
      let resolve: () => void = () => {}
      vi.mocked(remote.runAction).mockImplementationOnce(
        () => new Promise<void>((done) => (resolve = done))
      )
      let submitted: Promise<void> = Promise.resolve()
      act(() => {
        submitted = result.current.submit()
      })
      expect(remote.supersede).toHaveBeenCalledTimes(2)
      expect(remote.supersede).toHaveBeenLastCalledWith(first)
      // The FIRST send's row lands while the re-send is in flight.
      land()
      expect(result.current.text).toBe(``)
      expect(result.current.starting).toBeNull()
      expect(onLanded).toHaveBeenCalledTimes(1)
      await act(async () => {
        resolve()
        await submitted
      })
    } finally {
      vi.useRealTimers()
    }
  })

  it(`a run the watch had opened before the start is not its landing`, async () => {
    const remote = makeRemote({ openedId: `old-run` })
    const { result, rerender, onLanded } = mount(null, remote)
    act(() => result.current.setText(`hi`))
    await act(() => result.current.submit())
    rerender({ seed: null, remote: { ...remote, sentTo: null } })
    expect(result.current.text).toBe(`hi`)
    expect(onLanded).not.toHaveBeenCalled()
    rerender({ seed: null, remote: { ...remote, openedId: `new-run` } })
    expect(result.current.text).toBe(``)
    expect(onLanded).toHaveBeenCalledTimes(1)
  })
})

describe(`draftStartFingerprint`, () => {
  const base = {
    deviceId: `dev-1`,
    text: `hi`,
    subject: { kind: `issues` as const, ids: [`i1`, `i2`] },
    repoId: ``,
    attachments: [`blob:a.png`],
  }
  it(`is one start for the same draft on the same device`, () => {
    expect(
      draftStartFingerprint({
        ...base,
        subject: { kind: `issues`, ids: [`i2`, `i1`] },
      })
    ).toBe(draftStartFingerprint(base))
    const action = (inputs: Record<string, string>) =>
      draftStartFingerprint({
        ...base,
        subject: { kind: `action`, id: `act-1`, inputs },
      })
    expect(action({ a: `1`, b: `2` })).toBe(action({ b: `2`, a: `1` }))
  })
  it(`changes with the device, text, subject, attachments or stacked line`, () => {
    const fp = draftStartFingerprint(base)
    for (const change of [
      { deviceId: `dev-2` },
      { text: `hi!` },
      { subject: { kind: `issues` as const, ids: [`i1`] } },
      { subject: null },
      { attachments: [] },
      { stackedIssueId: `i1` },
    ]) {
      expect(draftStartFingerprint({ ...base, ...change })).not.toBe(fp)
    }
    expect(
      draftStartFingerprint({ ...base, subject: null, repoId: `r1` })
    ).not.toBe(draftStartFingerprint({ ...base, subject: null, repoId: `r2` }))
  })
})

// EXP-1233: the Fix merge conflicts builtin's own view — the picked PR off
// the synced rows (every issue a batch PR links), the submit label, and the
// refusal flag a refused merge seeds.
describe(`useLaunchComposer fix conflicts`, () => {
  const prIssue = (id: string, prUrl: string, extra: Record<string, unknown> = {}) => ({
    ...issue(id, `b1`, `in_review`),
    prUrl,
    prNumber: 2117,
    prState: `open`,
    branch: `exp/batch-1`,
    prBaseBranch: `master`,
    ...extra,
  })

  it(`is null for every other subject`, () => {
    const { result } = mount()
    expect(result.current.fixConflicts).toBeNull()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.fixConflicts).toBeNull()
    act(() => result.current.pickAction(`act-1`))
    expect(result.current.fixConflicts).toBeNull()
  })

  it(`resolves the picked PR's linked issues, branches and number`, () => {
    mockState.rows.issues = [
      prIssue(`i1`, `https://github.com/o/r/pull/2117`),
      prIssue(`i2`, `https://github.com/o/r/pull/2117`),
      prIssue(`i3`, `https://github.com/o/r/pull/3`, { prNumber: 3 }),
    ]
    const { result } = mount({
      issueIds: [],
      actionId: BUILTIN_FIX_CONFLICTS_ID,
      prIssueId: `i1`,
      conflict: true,
    })
    // Nothing picked yet (the card's seed latch does the pick): the generic
    // headline and "Run action" stay.
    expect(result.current.fixConflicts).toEqual({ pr: null, refused: true })
    expect(result.current.submitLabel).toBe(`Run action`)
    act(() => result.current.setInput(`pr`, `i1`))
    expect(result.current.fixConflicts?.pr).toEqual({
      issueId: `i1`,
      prNumber: 2117,
      branch: `exp/batch-1`,
      baseBranch: `master`,
      issues: [
        expect.objectContaining({ id: `i1` }),
        expect.objectContaining({ id: `i2` }),
      ],
    })
    expect(result.current.fixConflicts?.refused).toBe(true)
    expect(result.current.submitLabel).toBe(`Fix conflicts`)
    // Clearing the pick (the chip's ✕) keeps the builtin and the flag.
    act(() => result.current.setInput(`pr`, ``))
    expect(result.current.fixConflicts).toEqual({ pr: null, refused: true })
    // A different subject drops the refusal.
    act(() => result.current.pickAction(`act-1`))
    act(() => result.current.pickAction(BUILTIN_FIX_CONFLICTS_ID))
    expect(result.current.fixConflicts).toEqual({ pr: null, refused: false })
  })

  it(`a pick from the action picker is never flagged as refused`, () => {
    mockState.rows.issues = [prIssue(`i1`, `https://github.com/o/r/pull/2117`)]
    const { result } = mount()
    act(() => result.current.pickAction(BUILTIN_FIX_CONFLICTS_ID))
    act(() => result.current.setInput(`pr`, `i1`))
    expect(result.current.fixConflicts?.refused).toBe(false)
    expect(result.current.fixConflicts?.pr?.issues.map((row) => row.id)).toEqual([`i1`])
  })
})

describe(`useLaunchComposer seed`, () => {
  it(`consumes issues, then reports`, () => {
    const { result, onSeedConsumed } = mount({ issueIds: [`i1`, `i2`, `i1`] })
    expect(result.current.subject).toEqual({ kind: `issues`, ids: [`i1`, `i2`] })
    expect(onSeedConsumed).toHaveBeenCalledTimes(1)
  })

  it(`lets an action win over issues and carries the PR, icon and text`, () => {
    const { result } = mount({
      issueIds: [`i1`],
      actionId: BUILTIN_CREATE_ACTION_ID,
      prIssueId: `i9`,
      icon: `sparkles`,
      text: `Label new issues`,
    })
    expect(result.current.subject).toEqual({
      kind: `action`,
      id: BUILTIN_CREATE_ACTION_ID,
      inputs: { icon: `sparkles` },
    })
    expect(result.current.seedPrIssueId).toBe(`i9`)
    expect(result.current.text).toBe(`Label new issues`)
  })

  it(`never stomps typed text and applies a late seed`, async () => {
    const { result, rerender, onSeedConsumed } = mount(null)
    act(() => result.current.setText(`typing`))
    rerender({ seed: { issueIds: [`i3`], text: `seeded` } })
    await waitFor(() => expect(onSeedConsumed).toHaveBeenCalled())
    expect(result.current.text).toBe(`typing`)
    expect(result.current.subject).toEqual({ kind: `issues`, ids: [`i3`] })
  })

  it(`pre-picks the seeded device`, () => {
    const other: SteerDevice = { ...device, deviceId: `dev-2`, deviceLabel: `two`, rowId: `row-2` }
    const { result } = mount(
      { issueIds: [], deviceId: `dev-2` },
      makeRemote({ devices: [device, other] })
    )
    expect(result.current.launch.device?.deviceId).toBe(`dev-2`)
  })

  // EXP-836: the play button on a device row names a machine; it has to win
  // over the person's DEFAULT machine on every path the devices shape can
  // hydrate on, and it must not stick as a new default.
  it(`pre-picks a seeded device over the default machine, however late`, async () => {
    const mac: SteerDevice = {
      ...device,
      deviceId: `mac`,
      deviceLabel: `macbook`,
      rowId: `row-mac`,
      isDefault: true,
    }
    const mint: SteerDevice = {
      ...device,
      deviceId: `mint`,
      deviceLabel: `mint`,
      rowId: `row-mint`,
    }
    // a) seeded at mount.
    expect(
      mount({ issueIds: [], deviceId: `mint` }, makeRemote({ devices: [mac, mint] }))
        .result.current.launch.device?.deviceId
    ).toBe(`mint`)

    // b) seeded after mount (the route holds the seed in state and hands it
    // over on the next render).
    const late = mount(null, makeRemote({ devices: [mac, mint] }))
    expect(late.result.current.launch.device?.deviceId).toBe(`mac`)
    late.rerender({ seed: { issueIds: [], deviceId: `mint` } })
    await waitFor(() => expect(late.onSeedConsumed).toHaveBeenCalled())
    expect(late.result.current.launch.device?.deviceId).toBe(`mint`)
    expect(late.result.current.deviceRequestNote).toBeNull()
  })

  it(`holds the request until the devices shape hydrates`, async () => {
    const mac: SteerDevice = { ...device, deviceId: `mac`, isDefault: true, rowId: `row-mac` }
    const mint: SteerDevice = { ...device, deviceId: `mint`, deviceLabel: `mint`, rowId: `row-mint` }
    const onSeedConsumed = vi.fn()
    const hook = renderHook(
      ({ devices }: { devices: SteerDevice[] | null }) =>
        useLaunchComposer({
          teamId: `t1`,
          remote: makeRemote({ devices }),
          seed: { issueIds: [], deviceId: `mint` },
          onSeedConsumed,
        }),
      { initialProps: { devices: null as SteerDevice[] | null } }
    )
    // Nothing synced yet: no machine at all, the request still pending — and
    // NOT an accusation that the machine is gone.
    expect(hook.result.current.launch.device).toBeUndefined()
    expect(hook.result.current.deviceRequestNote).toBeNull()
    // The default machine lands FIRST, the requested one a beat later.
    hook.rerender({ devices: [mac] })
    expect(hook.result.current.launch.device?.deviceId).toBe(`mac`)
    hook.rerender({ devices: [mac, mint] })
    await waitFor(() =>
      expect(hook.result.current.launch.device?.deviceId).toBe(`mint`)
    )
  })

  it(`lets the select override the request, one-shot`, () => {
    const mac: SteerDevice = { ...device, deviceId: `mac`, isDefault: true, rowId: `row-mac` }
    const mint: SteerDevice = { ...device, deviceId: `mint`, rowId: `row-mint` }
    const { result } = mount(
      { issueIds: [], deviceId: `mint` },
      makeRemote({ devices: [mac, mint] })
    )
    act(() => result.current.launch.setDeviceId(`mac`))
    expect(result.current.launch.device?.deviceId).toBe(`mac`)
    // The seed does not reassert itself on the next devices poll…
    expect(result.current.launch.unavailableRequestId).toBeNull()
    // …and nothing writes the machine's `isDefault` flag either: picking is
    // local state, the default only moves through `devices.setDefault`.
    expect(mint.isDefault).toBeUndefined()
  })

  it(`explains a requested machine the composer cannot start on`, () => {
    const mac: SteerDevice = { ...device, deviceId: `mac`, isDefault: true, rowId: `row-mac` }
    // Registered, online, but every agent signed out: not a candidate.
    const mint: SteerDevice = {
      ...device,
      deviceId: `mint`,
      deviceLabel: `mint`,
      rowId: `row-mint`,
      agents: [],
    }
    const { result } = mount(
      { issueIds: [], deviceId: `mint` },
      makeRemote({ devices: [mac, mint] })
    )
    expect(result.current.launch.device?.deviceId).toBe(`mac`)
    expect(result.current.deviceRequestNote).toBe(
      `No agent is signed in on mint.`
    )

    // Offline says so instead.
    const offline = mount(
      { issueIds: [], deviceId: `mint` },
      makeRemote({ devices: [mac, { ...mint, agents: [`claude`], online: false }] })
    )
    expect(offline.result.current.deviceRequestNote).toBe(`mint is offline.`)

    // A machine that is not in the registry at all.
    const gone = mount(
      { issueIds: [], deviceId: `ghost` },
      makeRemote({ devices: [mac] })
    )
    expect(gone.result.current.deviceRequestNote).toBe(
      `That machine is no longer in your registry.`
    )
  })

  it(`seeds an action's repo input from its bound repository (EXP-349)`, () => {
    mockState.rows.a = [
      {
        ...action(`bound`, [
          { key: `repo`, label: `Repository`, type: `repo`, required: false },
        ]),
        repositoryId: `repo-1`,
      },
    ]
    const { result } = mount({ issueIds: [], actionId: `bound` })
    expect(result.current.subject).toEqual({
      kind: `action`,
      id: `bound`,
      inputs: { repo: `repo-1` },
    })
  })
})

// EXP-980: starting a BLOCKED issue asks first (SLOP-3: Cancel, Start
// anyway or Stacked PR).
describe(`useLaunchComposer blocked start`, () => {
  const blocks = (blocker: string, blocked: string) => ({
    type: `blocks`,
    issueId: blocker,
    relatedIssueId: blocked,
  })

  beforeEach(() => {
    mockState.rows.r = [blocks(`i2`, `i1`)]
    mockState.rows.bl = [issue(`i2`, `b1`, `in_progress`)]
  })

  it(`opens the dialog instead of starting a blocked issue`, async () => {
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.blockedStart.map((row) => row.id)).toEqual([`i2`])
    await act(() => result.current.submit())
    expect(result.current.blockedOpen).toBe(true)
    expect(remote.startIssues).not.toHaveBeenCalled()
  })

  it(`Start anyway starts the run`, async () => {
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    await act(() => result.current.submit())
    await act(() => result.current.startAnyway())
    expect(result.current.blockedOpen).toBe(false)
    expect(remote.startIssues).toHaveBeenCalledWith(
      device,
      expect.anything(),
      [`i1`],
      undefined,
      expect.any(Function),
      expect.any(String)
    )
  })

  // EXP-980: a batch asks too — about what blocks it from OUTSIDE.
  it(`asks for a batch that an outside issue blocks`, async () => {
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.toggleIssue(`i3`))
    expect(result.current.blockedStart.map((row) => row.id)).toEqual([`i2`])
    await act(() => result.current.submit())
    expect(result.current.blockedOpen).toBe(true)
    expect(remote.startIssues).not.toHaveBeenCalled()
    await act(() => result.current.startAnyway())
    expect(remote.startIssues).toHaveBeenCalledWith(
      device,
      expect.anything(),
      [`i1`, `i3`],
      undefined,
      expect.any(Function),
      expect.any(String)
    )
  })

  it(`never asks about a blocker picked into the same batch`, async () => {
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.toggleIssue(`i2`))
    expect(result.current.blockedStart).toEqual([])
    await act(() => result.current.submit())
    expect(result.current.blockedOpen).toBe(false)
    expect(remote.startIssues).toHaveBeenCalledTimes(1)
  })

  it(`never asks on a resume`, async () => {
    mockState.rows.w = [
      {
        deviceRowId: device.rowId,
        issueIdentifier: `I1`,
        agents: null,
      },
    ]
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.resumeActive).toBe(true)
    await act(() => result.current.submit())
    expect(result.current.blockedOpen).toBe(false)
    expect(remote.startIssues).toHaveBeenCalledTimes(1)
  })

  // SLOP-3: the third button builds the dependency LINE bottom-up.
  it(`disables Stacked PR while a line member is already running`, () => {
    mockState.rows.s = [
      { issueId: `i2`, status: `running`, teamId: `t1`, updatedAt: new Date() },
    ]
    const { result } = mount()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.blockedStack).toMatchObject({
      plan: null,
      reason: `running`,
      ident: `I2`,
    })
  })

  it(`Stacked PR on an open blocker PR starts the picked issue with the base instruction`, async () => {
    mockState.rows.bl = [
      { ...issue(`i2`, `b2`, `in_progress`), prState: `open`, branch: `exp/I2` },
    ]
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.blockedStack.reason).toBeNull()
    expect(result.current.blockedStack.first?.id).toBe(`i1`)
    act(() => result.current.setText(`keep it small`))
    await act(() => result.current.submit())
    await act(() => result.current.startStacked())
    expect(result.current.blockedOpen).toBe(false)
    expect(remote.startIssues).toHaveBeenCalledWith(
      device,
      expect.not.objectContaining({ resume: true }),
      [`i1`],
      stackedStartPrompt(
        { base: { identifier: `I2`, branch: `exp/I2` }, run: [`I1`] },
        `keep it small`
      ),
      expect.any(Function),
      expect.any(String)
    )
  })

  it(`Stacked PR on a line without PRs starts its BOTTOM issue`, async () => {
    // i3 blocks i2 blocks i1: the line under i1 is [I3, I2].
    mockState.rows.tr = [blocks(`i2`, `i1`), blocks(`i3`, `i2`)]
    mockState.rows.li = [
      issue(`i1`, `b1`),
      issue(`i2`, `b1`, `in_progress`),
      issue(`i3`, `b2`),
    ]
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.blockedStack.plan).toEqual({
      base: null,
      run: [`I3`, `I2`, `I1`],
    })
    expect(result.current.blockedStack.first?.id).toBe(`i3`)
    await act(() => result.current.submit())
    await act(() => result.current.startStacked())
    expect(remote.startIssues).toHaveBeenCalledWith(
      device,
      expect.anything(),
      [`i3`],
      stackedStartPrompt({ base: null, run: [`I3`, `I2`, `I1`] }, ``),
      expect.any(Function),
      expect.any(String)
    )
  })

  it(`disables Stacked PR for a line member in another repository`, () => {
    mockState.boards = [board(`b1`, `repo-1`), board(`b2`, `repo-2`)]
    mockState.rows.bl = [
      { ...issue(`i2`, `b2`, `in_progress`), prState: `open`, branch: `exp/I2` },
    ]
    const { result } = mount()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.blockedStack).toMatchObject({
      plan: null,
      reason: `repo`,
      ident: `I2`,
    })
  })

  it(`never asks for a done blocker`, async () => {
    mockState.rows.bl = [issue(`i2`, `b1`, `done`)]
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    expect(result.current.blockedStart).toEqual([])
    await act(() => result.current.submit())
    expect(result.current.blockedOpen).toBe(false)
    expect(remote.startIssues).toHaveBeenCalledTimes(1)
  })
})

// EXP-792: the server holds each member's MCP credential, so a server is
// usable when the CALLER's connection is `connected` or `not_needed` — on any
// machine. An unconnected one is never preselected, even when the team marked
// it enabled-by-default, and the picker sends the caller to connect it.
describe(`useLaunchComposer MCP servers`, () => {
  const server = (
    id: string,
    status: string,
    enabledByDefault: boolean
  ) => ({
    id,
    name: id,
    enabledByDefault,
    connection: { status, expiresAt: null, error: null },
  })

  it(`preselects only the default servers the caller can use`, async () => {
    mockState.mcpServers = [
      server(`linear`, `connected`, true),
      server(`sentry`, `not_connected`, true),
      server(`notion`, `expired`, true),
      server(`docs`, `not_needed`, true),
      server(`stripe`, `connected`, false),
    ]
    const { result } = mount()
    await waitFor(() =>
      expect(result.current.launch.mcpServerIds).toEqual([`linear`, `docs`])
    )
    expect(result.current.mcpConnectHref(`sentry`)).toBe(
      `/t/acme/settings/mcp-servers?connect=sentry`
    )
  })
})

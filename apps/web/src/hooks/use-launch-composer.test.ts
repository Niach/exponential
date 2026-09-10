import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerDevice } from "@/lib/steer-devices"
import {
  BUILTIN_CHAT_ID,
  BUILTIN_CREATE_ACTION_ID,
  BUILTIN_FIX_CONFLICTS_ID,
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
  } as Record<string, unknown[]>,
  boards: [] as unknown[],
  repos: null as { id: string; fullName: string }[] | null,
  upload: vi.fn(),
  toastError: vi.fn(),
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
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => mockState.boards,
}))
vi.mock(`@/hooks/use-team-repos`, () => ({
  useTeamRepos: () => mockState.repos,
}))
vi.mock(`@/hooks/use-mcp-servers`, () => ({
  useMcpServers: () => ({ servers: null, error: null, refresh: vi.fn() }),
}))
vi.mock(`@/lib/storage/issue-image-upload`, () => ({
  uploadTeamSessionImageFile: mockState.upload,
}))
vi.mock(`sonner`, () => ({
  toast: { error: mockState.toastError },
}))

import { useLaunchComposer, type LaunchSeed } from "@/hooks/use-launch-composer"
import type { RemoteStart } from "@/hooks/use-remote-start"

const device: SteerDevice = {
  rowId: `row-1`,
  deviceId: `dev-1`,
  deviceLabel: `buildbox`,
  agents: [`claude`],
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
    refresh: vi.fn(),
    latestVersions: null,
    ...overrides,
  }
}

function mount(seed: LaunchSeed | null = null, remote = makeRemote()) {
  const onSeedConsumed = vi.fn()
  const hook = renderHook(
    ({ seed: current }) =>
      useLaunchComposer({ teamId: `t1`, remote, seed: current, onSeedConsumed }),
    { initialProps: { seed } }
  )
  return { ...hook, remote, onSeedConsumed }
}

beforeEach(() => {
  mockState.rows.a = [action(`act-1`)]
  mockState.rows.issues = [issue(`i1`, `b1`), issue(`i2`, `b1`), issue(`i3`, `b2`)]
  mockState.rows.s = []
  mockState.rows.w = []
  mockState.boards = [board(`b1`, `repo-1`), board(`b2`, `repo-1`)]
  mockState.repos = [{ id: `repo-1`, fullName: `acme/app` }]
  mockState.upload.mockReset()
  mockState.toastError.mockReset()
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

  it(`lists fix-conflicts, then Create action, then the team's rows`, () => {
    const { result } = mount()
    expect(result.current.actions?.map((a) => a.id)).toEqual([
      BUILTIN_FIX_CONFLICTS_ID,
      BUILTIN_CREATE_ACTION_ID,
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
    const { result, remote } = mount()
    act(() => result.current.setText(`  Explain #I1  `))
    await act(() => result.current.submit())
    expect(remote.runAction).toHaveBeenCalledWith(
      device,
      { id: BUILTIN_CHAT_ID, name: `Chat`, teamId: `t1` },
      expect.objectContaining({ agent: `claude`, planMode: false }),
      // The only repo pre-picks itself (EXP-822) and rides as the `repo` input.
      { repo: `repo-1` },
      `Explain #I1`
    )
    expect(result.current.text).toBe(``)
    expect(result.current.subject).toBeNull()
  })

  it(`starts issues with the text as additional instructions`, async () => {
    const { result, remote } = mount()
    act(() => result.current.toggleIssue(`i1`))
    act(() => result.current.toggleIssue(`i2`))
    act(() => result.current.setText(`Keep the API stable`))
    await act(() => result.current.submit())
    expect(remote.startIssues).toHaveBeenCalledWith(
      device,
      expect.objectContaining({ agent: `claude`, planMode: true }),
      [`i1`, `i2`],
      `Keep the API stable`
    )
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
      undefined
    )
  })

  it(`uploads images sequentially and embeds them in the prompt`, async () => {
    mockState.upload
      .mockResolvedValueOnce({ id: `att-1` })
      .mockResolvedValueOnce({ id: `att-2` })
    const { result, remote } = mount()
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
      `see [Image #1] [Image #2]\n\n![image](/api/attachments/att-1)\n![image](/api/attachments/att-2)`
    )
    expect(result.current.images).toEqual([])
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
      `Couldn't upload image`,
      expect.anything()
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

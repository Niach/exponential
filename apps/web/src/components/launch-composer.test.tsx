import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue } from "@/db/schema"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import type { LaunchOptions } from "@/components/launch-dialog/use-launch-options"
import type { SteerDevice } from "@/lib/steer-devices"
import { builtinCreateAction, builtinFixConflictsAction } from "@/lib/builtin-actions"
import { CHAT_SUGGESTION_COUNT, CHAT_SUGGESTION_POOL } from "@/lib/chat-suggestions"

// EXP-825: the composer card over a FAKE model — chips, the per-subject
// submit label and placeholder, Enter-sends, the suggestion pills. The hook
// has its own tests; this only proves the chrome reads the model.

vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})
vi.mock(`@/lib/collections`, () => ({
  actionCollection: {},
  boardCollection: {},
  codingSessionCollection: {},
  deviceWorktreeCollection: {},
  issueCollection: {},
}))
vi.mock(`@/hooks/use-mobile`, () => ({ useIsMobile: () => false }))

import {
  LaunchComposer,
  suggestionCaretOffset,
} from "@/components/launch-composer"

const device: SteerDevice = {
  deviceId: `dev-1`,
  deviceLabel: `buildbox`,
  agents: [`claude`],
  online: true,
}

const issue = (id: string, identifier: string): Issue =>
  ({
    id,
    identifier,
    title: `Issue ${identifier}`,
    status: `backlog`,
    priority: `none`,
    boardId: `b1`,
  }) as unknown as Issue

function fakeLaunch(): LaunchOptions {
  return {
    device,
    deviceId: device.deviceId,
    setDeviceId: vi.fn(),
    requestDevice: vi.fn(),
    unavailableRequestId: null,
    availableAgents: [`claude`],
    agent: `claude`,
    agentNotReady: false,
    switchAgent: vi.fn(),
    model: `opus`,
    setModel: vi.fn(),
    effortValue: `cli-default`,
    setEffortValue: vi.fn(),
    ultracode: false,
    setUltracode: vi.fn(),
    planMode: false,
    setPlanMode: vi.fn(),
    mcpServerIds: [],
    setMcpServerIds: vi.fn(),
    toggleMcpServer: vi.fn(),
    accountProfiles: [],
    account: undefined,
    setAccount: vi.fn(),
    buildOptions: () => ({
      agent: `claude`,
      model: `opus`,
      effort: ``,
      ultracode: false,
      planMode: false,
    }),
  }
}

function fakeModel(overrides: Partial<LaunchComposerModel> = {}): LaunchComposerModel {
  return {
    teamId: `t1`,
    subject: null,
    selectedAction: null,
    actions: [builtinFixConflictsAction(`t1`), builtinCreateAction(`t1`)],
    eligibleIssues: [],
    checkedIssues: [],
    toggleIssue: vi.fn(),
    pickAction: vi.fn(),
    clearAction: vi.fn(),
    setInput: vi.fn(),
    seedPrIssueId: undefined,
    text: ``,
    setText: vi.fn(),
    images: [],
    addFiles: vi.fn(() => 0),
    removeImage: vi.fn(),
    repos: [],
    repoId: ``,
    setRepoId: vi.fn(),
    repoOptions: [],
    resumeCandidate: null,
    resume: true,
    setResume: vi.fn(),
    resumeActive: false,
    launch: fakeLaunch(),
    candidateDevices: [device],
    deviceRequestNote: null,
    mcpServers: null,
    mcpNow: new Date(),
    submitLabel: `Start chat`,
    blocked: true,
    overCap: false,
    spansRepos: false,
    spansBranches: false,
    costHint: false,
    busy: false,
    sentTo: null,
    submit: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  }
}

describe(`LaunchComposer`, () => {
  it(`renders the chat state: placeholder, suggestions, an icon-only submit`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    expect(screen.getByTestId(`agent-composer`)).toBeTruthy()
    expect(screen.getByPlaceholderText(`Ask the agent…`)).toBeTruthy()
    const submit = screen.getByTestId(`agent-composer-submit`) as HTMLButtonElement
    // EXP-827: the label is the button's NAME, never its text.
    expect(submit.getAttribute(`aria-label`)).toBe(`Start chat`)
    expect(submit.textContent).toBe(``)
    expect(submit.disabled).toBe(true)
    // EXP-820: a few pool chips over the empty field (a random draw).
    const pills = screen
      .getAllByRole(`button`)
      .filter((button) => CHAT_SUGGESTION_POOL.includes(button.textContent ?? ``))
    expect(pills.length).toBe(CHAT_SUGGESTION_COUNT)
    expect(screen.queryByTestId(/agent-composer-chip-/)).toBeNull()
  })

  it(`draws one chip per checked issue and lets a chip remove itself`, () => {
    const model = fakeModel({
      subject: { kind: `issues`, ids: [`i1`, `i2`] },
      checkedIssues: [issue(`i1`, `APP-3`), issue(`i2`, `APP-6`)],
      submitLabel: `Start batch · 2`,
      blocked: false,
    })
    render(<LaunchComposer model={model} users={[]} />)
    expect(screen.getByTestId(`agent-composer-chip-issue-APP-3`)).toBeTruthy()
    expect(screen.getByTestId(`agent-composer-chip-issue-APP-6`)).toBeTruthy()
    expect(
      screen.getByTestId(`agent-composer-submit`).getAttribute(`aria-label`)
    ).toBe(`Start batch · 2`)
    // Text is optional once there is a subject — the field says so.
    expect(screen.getByPlaceholderText(`Additional instructions (optional)…`)).toBeTruthy()
    // EXP-827: the chip body is the issue pill (inert without an issue-ref
    // provider); only its ✕ removes.
    fireEvent.click(screen.getByTestId(`agent-composer-chip-issue-APP-3`))
    expect(model.toggleIssue).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`agent-composer-chip-issue-APP-3-remove`))
    expect(model.toggleIssue).toHaveBeenCalledWith(`i1`)
  })

  it(`draws the action chip and asks for the request on Create action`, () => {
    const create = builtinCreateAction(`t1`)
    const model = fakeModel({
      subject: { kind: `action`, id: create.id, inputs: {} },
      selectedAction: create,
      submitLabel: `Run action`,
    })
    render(<LaunchComposer model={model} users={[]} />)
    const chip = screen.getByTestId(`agent-composer-chip-action`)
    expect(chip.textContent).toContain(`Create action`)
    expect(screen.getByPlaceholderText(/Describe the action/)).toBeTruthy()
    // EXP-827: clicking the chip keeps the action; its ✕ clears it.
    fireEvent.click(chip)
    expect(model.clearAction).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`agent-composer-chip-action-remove`))
    expect(model.clearAction).toHaveBeenCalled()
  })

  // EXP-825: an action's own hint replaces the generic one while it is picked.
  it(`shows the picked action's composer hint as the field placeholder`, () => {
    const fix = builtinFixConflictsAction(`t1`)
    const model = fakeModel({
      subject: { kind: `action`, id: fix.id, inputs: {} },
      selectedAction: { ...fix, promptPlaceholder: `Scope: which platforms` },
      submitLabel: `Run action`,
    })
    render(<LaunchComposer model={model} users={[]} />)
    expect(screen.getByPlaceholderText(`Scope: which platforms`)).toBeTruthy()

    const plain = fakeModel({
      subject: { kind: `action`, id: fix.id, inputs: {} },
      selectedAction: fix,
      submitLabel: `Run action`,
    })
    render(<LaunchComposer model={plain} users={[]} />)
    expect(screen.getByPlaceholderText(/Additional instructions/)).toBeTruthy()
  })

  it(`Enter sends when not blocked, Shift+Enter breaks the line`, () => {
    const model = fakeModel({ text: `hello`, blocked: false })
    render(<LaunchComposer model={model} users={[]} />)
    const field = screen.getByTestId(`agent-composer-field`)
    fireEvent.keyDown(field, { key: `Enter`, shiftKey: true })
    expect(model.submit).not.toHaveBeenCalled()
    fireEvent.keyDown(field, { key: `Enter` })
    expect(model.submit).toHaveBeenCalledTimes(1)
  })

  it(`never sends while blocked`, () => {
    const model = fakeModel({ blocked: true })
    render(<LaunchComposer model={model} users={[]} />)
    fireEvent.keyDown(screen.getByTestId(`agent-composer-field`), { key: `Enter` })
    expect(model.submit).not.toHaveBeenCalled()
  })

  it(`forwards typing to the model`, () => {
    const model = fakeModel()
    render(<LaunchComposer model={model} users={[]} />)
    fireEvent.change(screen.getByTestId(`agent-composer-field`), {
      target: { value: `Fix #` },
    })
    expect(model.setText).toHaveBeenCalledWith(`Fix #`)
  })

  // EXP-827: a `#` suggestion parks the caret right behind the `#`, wherever
  // it sits, so the issue-ref menu opens without a backspace.
  it(`lands the caret after a suggestion's # placeholder`, () => {
    expect(suggestionCaretOffset(`Fix #`)).toBe(5)
    expect(suggestionCaretOffset(`Start a session for # on my other machine`)).toBe(21)
    expect(suggestionCaretOffset(`Label every issue in the backlog`)).toBeUndefined()
  })

  it(`says so when no desktop is online`, () => {
    render(
      <LaunchComposer model={fakeModel({ candidateDevices: [] })} users={[]} />
    )
    expect(screen.getByTestId(`agent-options-row`).textContent).toContain(
      `No desktop online`
    )
  })
})

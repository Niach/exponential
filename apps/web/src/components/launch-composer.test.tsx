import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue } from "@/db/schema"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import type { LaunchOptions } from "@/components/launch-dialog/use-launch-options"
import type { SteerDevice } from "@/lib/steer-devices"
import { builtinCreateAction, builtinFixConflictsAction } from "@/lib/builtin-actions"
import { CHAT_SUGGESTION_COUNT, CHAT_SUGGESTION_POOL } from "@/lib/chat-suggestions"
import {
  BLOCKED_BATCH_BODY,
  BLOCKED_BATCH_TITLE,
  BLOCKED_START_TITLE,
  STACKED_PR_LABEL,
  START_ANYWAY_LABEL,
} from "@/lib/blocked-start"

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
// EXP-980: the dialog's mini-graph reads the team graph; the hook has its own
// consumers' tests, here it only has to feed one blocks edge (or a cycle).
const graphState = vi.hoisted(() => ({
  relations: [] as { type: string; issueId: string; relatedIssueId: string }[],
  issues: [] as unknown[],
}))
vi.mock(`@/hooks/use-team-issue-graph`, () => ({
  useTeamIssueGraph: () => ({ ...graphState, counts: new Map() }),
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => false,
}))
// The shared issue engine is fixture-tested on its own; the "+" menu's
// Implement submenu only has to list the pool it is handed.
vi.mock(`@/hooks/use-issue-search-results`, () => ({
  useIssueSearchResults: ({ rows }: { rows: unknown[] }) => ({ results: rows }),
}))

import {
  LaunchComposer,
  suggestionCaretOffset,
} from "@/components/launch-composer"

// Radix positions the popovers with ResizeObserver and cmdk scrolls the
// active row into view; jsdom has neither.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

// Radix opens a dropdown on the trigger's pointer DOWN.
function openPlus() {
  act(() => {
    fireEvent.pointerDown(screen.getByTestId(`agent-composer-plus-button`), {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

const menuRowIds = () =>
  Array.from(
    screen
      .getByTestId(`agent-composer-menu`)
      .querySelectorAll(`[data-testid^="agent-composer-menu-"], [role=separator]`)
  ).map((node) =>
    node.getAttribute(`role`) === `separator`
      ? `-`
      : node.getAttribute(`data-testid`)!.replace(`agent-composer-menu-`, ``)
  )

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
    subagentModel: ``,
    setSubagentModel: vi.fn(),
    effortValue: `cli-default`,
    setEffortValue: vi.fn(),
    ultracode: false,
    setUltracode: vi.fn(),
    planMode: false,
    setPlanMode: vi.fn(),
    mcpServerIds: [],
    setMcpServerIds: vi.fn(),
    toggleMcpServer: vi.fn(),
    computerUseAvailable: false,
    computerUse: false,
    setComputerUse: vi.fn(),
    accountOptions: [],
    accountKey: undefined,
    setAccountKey: vi.fn(),
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
    fixConflicts: null,
    text: ``,
    setText: vi.fn(),
    images: [],
    addFiles: vi.fn(() => 0),
    acceptsFiles: true,
    removeImage: vi.fn(),
    repos: [],
    repoId: ``,
    setRepoId: vi.fn(),
    repoOptions: [],
    resumeCandidate: null,
    resume: true,
    setResume: vi.fn(),
    resumeActive: false,
    blockedStart: [],
    blockedOpen: false,
    closeBlockedStart: vi.fn(),
    startAnyway: vi.fn().mockResolvedValue(undefined),
    blockedStack: { plan: null, reason: null, ident: null, first: null },
    startStacked: vi.fn().mockResolvedValue(undefined),
    launch: fakeLaunch(),
    candidateDevices: [device],
    deviceRequestNote: null,
    mcpServers: null,
    mcpConnectHref: () => null,
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
    // EXP-820/EXP-1249: a few pool entries as QUIET rows (a random draw),
    // under the options line — never pills over the card.
    const rows = screen.getAllByTestId(`agent-composer-suggestion`)
    expect(rows.length).toBe(CHAT_SUGGESTION_COUNT)
    for (const row of rows) {
      expect(CHAT_SUGGESTION_POOL).toContain(row.textContent)
      expect(row.getAttribute(`data-slot`)).not.toBe(`pill`)
    }
    const options = screen.getByTestId(`agent-options-row`)
    expect(
      options.compareDocumentPosition(rows[0]!) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
    expect(screen.queryByTestId(/agent-composer-chip-/)).toBeNull()
  })

  it(`inserts a suggestion into the field and hides the rows once there is a subject`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    expect(screen.getAllByTestId(`agent-composer-suggestion`).length).toBeGreaterThan(0)
    render(
      <LaunchComposer
        model={fakeModel({
          subject: { kind: `issues`, ids: [`i1`] },
          checkedIssues: [issue(`i1`, `APP-1`)],
        })}
        users={[]}
      />
    )
    // Only the first render's rows remain.
    expect(screen.getAllByTestId(`agent-composer-suggestion`).length).toBe(
      CHAT_SUGGESTION_COUNT
    )
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

  // EXP-1233: the Fix merge conflicts builtin wears its own card once a
  // pull request is picked — the PR's issue chips in the headline, the
  // branch row (the picker) in the strip, and the refusal line when a
  // refused merge opened the composer. No generic "Pull request" field.
  it(`draws the conflict card for a picked Fix merge conflicts PR`, () => {
    const fix = builtinFixConflictsAction(`t1`)
    const pr = {
      issueId: `i1`,
      prNumber: 2117,
      branch: `exp/APP-14`,
      baseBranch: `master`,
      issues: [issue(`i1`, `APP-14`)],
    }
    const model = fakeModel({
      subject: { kind: `action`, id: fix.id, inputs: { pr: `i1` } },
      selectedAction: fix,
      fixConflicts: { pr, refused: true },
      submitLabel: `Fix conflicts`,
      blocked: false,
    })
    render(<LaunchComposer model={model} users={[]} />)
    const headline = screen.getByTestId(`agent-composer-headline`)
    expect(headline.textContent).toContain(`Fix merge conflicts`)
    expect(headline.textContent).not.toContain(`Run`)
    expect(screen.getByTestId(`agent-composer-chip-issue-APP-14`)).toBeTruthy()
    expect(screen.queryByTestId(`agent-composer-chip-action`)).toBeNull()
    const card = screen.getByTestId(`agent-composer-fix-conflicts`)
    expect(card.textContent).toContain(`#2117`)
    expect(card.textContent).toContain(`exp/APP-14 → master`)
    expect(screen.getByTestId(`agent-composer-conflict-note`).textContent).toBe(
      `Merge refused: the branch has conflicts.`
    )
    expect(screen.queryByText(`Pull request`)).toBeNull()
    expect(
      screen.getByTestId(`agent-composer-submit`).getAttribute(`aria-label`)
    ).toBe(`Fix conflicts`)
    // The chip's ✕ clears the PICK, not the action.
    fireEvent.click(screen.getByTestId(`agent-composer-chip-issue-APP-14-remove`))
    expect(model.setInput).toHaveBeenCalledWith(`pr`, ``)
    expect(model.clearAction).not.toHaveBeenCalled()
  })

  it(`keeps the generic action chip and the picker while no PR is picked`, () => {
    const fix = builtinFixConflictsAction(`t1`)
    const model = fakeModel({
      subject: { kind: `action`, id: fix.id, inputs: {} },
      selectedAction: fix,
      fixConflicts: { pr: null, refused: false },
      submitLabel: `Run action`,
    })
    render(<LaunchComposer model={model} users={[]} />)
    expect(screen.getByTestId(`agent-composer-headline`).textContent).toContain(`Run`)
    expect(screen.getByTestId(`agent-composer-chip-action`)).toBeTruthy()
    const card = screen.getByTestId(`agent-composer-fix-conflicts`)
    expect(card.textContent).toContain(`Select a pull request…`)
    expect(screen.queryByTestId(`agent-composer-conflict-note`)).toBeNull()
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

  // EXP-872: the options line — ONE account picker over the machine's
  // flattened logins (brand mark + email, never a profile name), the agent
  // implied by the pick; one login collapses to plain text.
  it(`says the one login as plain text`, () => {
    const model = fakeModel({
      launch: {
        ...fakeLaunch(),
        accountKey: `claude:system`,
        accountOptions: [
          { id: `system`, agent: `claude`, email: `me@example.com`, isLastUsed: true, health: `ok` },
        ],
      },
    })
    render(<LaunchComposer model={model} users={[]} />)
    const word = screen.getByTestId(`agent-composer-account`)
    expect(word.getAttribute(`data-slot`)).toBe(`combobox-inline-word`)
    expect(word.textContent).toContain(`me@example.com`)
    expect(screen.queryByLabelText(`Account`)).toBeNull()
  })

  it(`offers every login by email and reports the pick`, () => {
    const setAccountKey = vi.fn()
    const model = fakeModel({
      launch: {
        ...fakeLaunch(),
        setAccountKey,
        accountKey: `claude:p1`,
        accountOptions: [
          { id: `p1`, agent: `claude`, email: `work@example.com`, isLastUsed: true, health: `ok` },
          { id: `only`, agent: `codex`, email: `codex@example.com`, isLastUsed: false, health: `needs_relogin` },
        ],
      },
    })
    render(<LaunchComposer model={model} users={[]} />)
    const account = screen.getByLabelText(`Account`)
    expect(account.textContent).toContain(`work@example.com`)
    expect(account.textContent?.toLowerCase()).not.toContain(`default`)
    fireEvent.click(account)
    expect(screen.getByText(`codex@example.com`)).toBeTruthy()
    expect(screen.getByText(`Needs re-login`)).toBeTruthy()
    fireEvent.click(screen.getByText(`codex@example.com`))
    expect(setAccountKey).toHaveBeenCalledWith(`codex:only`)
    // The row is where it lives — never the overflow sheet.
    expect(screen.queryByTestId(`agent-options-sheet`)).toBeNull()
  })

  // EXP-1249: Run action › hosts THE action picker (@exp/ui `PickerList`)
  // — one action row, curated glyph over the name, description underneath;
  // a pick sets the subject and closes the menu.
  it(`picks an action through the + menu's Run action submenu`, () => {
    const fix = builtinFixConflictsAction(`t1`)
    const model = fakeModel()
    render(<LaunchComposer model={model} users={[]} />)
    openPlus()
    fireEvent.click(screen.getByTestId(`agent-composer-menu-run-action`))
    const picker = screen.getByTestId(`agent-composer-actions-picker`)
    const row = within(picker)
      .getByText(fix.name)
      .closest(`[data-slot=command-item]`)!
    expect(
      row.querySelector(`[data-slot=picker-description]`)?.textContent
    ).toBe(fix.description)
    fireEvent.click(row)
    expect(model.pickAction).toHaveBeenCalledWith(fix.id)
    expect(screen.queryByTestId(`agent-composer-menu`)).toBeNull()
  })

  // EXP-1030: the machine is picked through THE device picker (@exp/ui
  // `DevicePicker` on the shared primitive), as an inline word of the line.
  it(`picks the machine through the shared device picker`, () => {
    const second: SteerDevice = {
      deviceId: `dev-2`,
      deviceLabel: `the mini`,
      agents: [`claude`],
      online: true,
      kind: `server`,
    }
    const setDeviceId = vi.fn()
    render(
      <LaunchComposer
        model={fakeModel({
          candidateDevices: [device, second],
          launch: { ...fakeLaunch(), setDeviceId },
        })}
        users={[]}
      />
    )
    const trigger = screen.getByLabelText(`Device`)
    expect(trigger.getAttribute(`data-slot`)).toBe(`combobox-inline-trigger`)
    expect(trigger.textContent).toContain(`buildbox`)
    // The trigger hangs under the primitive's marker — a single pick.
    expect(
      trigger.closest(`[data-slot="picker"]`)?.getAttribute(`data-picker-mode`)
    ).toBe(`single`)
    fireEvent.click(trigger)
    fireEvent.click(screen.getByText(`the mini`))
    expect(setDeviceId).toHaveBeenCalledWith(`dev-2`)
  })

  // One machine is not a choice: the sentence says it, with no chevron.
  it(`says the one machine as plain text`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    expect(screen.queryByLabelText(`Device`)).toBeNull()
    const word = document.querySelector(`[title="Device"]`)
    expect(word?.getAttribute(`data-slot`)).toBe(`combobox-inline-word`)
    expect(word?.textContent).toContain(`buildbox`)
  })

  // EXP-993: the repository is a choice only with several repos, and there
  // is no repo-less row.
  it(`shows the repository picker only with two or more repos`, () => {
    const one = fakeModel({
      repoId: `r1`,
      repoOptions: [{ value: `r1`, label: `niach/exponential` }],
    })
    render(<LaunchComposer model={one} users={[]} />)
    expect(screen.queryByLabelText(`Repository`)).toBeNull()

    const two = fakeModel({
      repoId: `r1`,
      repoOptions: [
        { value: `r1`, label: `niach/exponential` },
        { value: `r2`, label: `niach/other` },
      ],
    })
    render(<LaunchComposer model={two} users={[]} />)
    const repo = screen.getByLabelText(`Repository`)
    expect(repo.textContent).toContain(`niach/exponential`)
    fireEvent.click(repo)
    expect(screen.queryByText(`No repository`)).toBeNull()
  })

  // EXP-1249: the `⋯` overflow is gone — its rows moved into the "+".
  it(`draws no overflow on the options line`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    expect(screen.queryByLabelText(`More options`)).toBeNull()
    expect(screen.queryByTestId(`agent-options-sheet`)).toBeNull()
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

// EXP-1249: the composer's ONE tool — the "+" menu (composer-menu.json).
describe(`LaunchComposer + menu`, () => {
  it(`is the only tool: no Issue, Action or image buttons`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    expect(screen.getByTestId(`agent-composer-plus-button`)).toBeTruthy()
    expect(screen.queryByTestId(`agent-composer-issues-button`)).toBeNull()
    expect(screen.queryByTestId(`agent-composer-actions-button`)).toBeNull()
    expect(screen.queryByTestId(`agent-composer-image-button`)).toBeNull()
  })

  it(`lists the fixture's rows for claude without MCP servers or computer use`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    openPlus()
    expect(menuRowIds()).toEqual([
      `implement-issue`,
      `run-action`,
      `add-file`,
      `-`,
      `effort`,
      `subagents`,
      `ultracode`,
    ])
    expect(screen.getByTestId(`agent-composer-menu-effort`).textContent).toContain(
      `CLI default`
    )
  })

  it(`adds MCP servers and Computer use when the team and the device allow them`, () => {
    const setComputerUse = vi.fn()
    const toggleMcpServer = vi.fn()
    const model = fakeModel({
      mcpServers: [
        {
          id: `m1`,
          name: `Linear`,
          url: `https://mcp.linear.app/mcp`,
          command: null,
          enabledByDefault: false,
          connection: { state: `connected` },
        },
      ] as never,
      launch: {
        ...fakeLaunch(),
        computerUseAvailable: true,
        computerUse: false,
        setComputerUse,
        mcpServerIds: [`m1`],
        toggleMcpServer,
      },
    })
    render(<LaunchComposer model={model} users={[]} />)
    openPlus()
    expect(menuRowIds().slice(-3)).toEqual([`-`, `mcp-servers`, `computer-use`])
    // The picked count rides the row as its value.
    expect(
      screen.getByTestId(`agent-composer-menu-mcp-servers`).textContent
    ).toContain(`1`)
    const computerUse = screen.getByTestId(`agent-composer-menu-computer-use`)
    expect(computerUse.getAttribute(`role`)).toBe(`menuitemcheckbox`)
    expect(computerUse.getAttribute(`aria-checked`)).toBe(`false`)
    fireEvent.click(computerUse)
    expect(setComputerUse).toHaveBeenCalledWith(true)
    // A toggle keeps the menu open.
    expect(screen.getByTestId(`agent-composer-menu`)).toBeTruthy()
  })

  it(`hides the MCP row while a real action owns its MCP list`, () => {
    const model = fakeModel({
      subject: { kind: `action`, id: `a-real`, inputs: {} },
      mcpServers: [{ id: `m1`, name: `Linear`, connection: { state: `connected` } }] as never,
    })
    render(<LaunchComposer model={model} users={[]} />)
    openPlus()
    expect(menuRowIds()).not.toContain(`mcp-servers`)
  })

  it(`says Reasoning and drops the claude-only rows for codex`, () => {
    const model = fakeModel({ launch: { ...fakeLaunch(), agent: `codex` } })
    render(<LaunchComposer model={model} users={[]} />)
    openPlus()
    expect(menuRowIds()).toEqual([`implement-issue`, `run-action`, `add-file`, `-`, `effort`])
    expect(screen.getByTestId(`agent-composer-menu-effort`).textContent).toContain(
      `Reasoning`
    )
  })

  it(`toggles ultracode and disables Effort while it is on`, () => {
    const setUltracode = vi.fn()
    render(
      <LaunchComposer
        model={fakeModel({
          launch: { ...fakeLaunch(), ultracode: true, setUltracode },
        })}
        users={[]}
      />
    )
    openPlus()
    expect(
      screen.getByTestId(`agent-composer-menu-effort`).hasAttribute(`data-disabled`)
    ).toBe(true)
    fireEvent.click(screen.getByTestId(`agent-composer-menu-ultracode`))
    expect(setUltracode).toHaveBeenCalledWith(false)
  })

  it(`picks issues in the Implement submenu and offers the implement button`, () => {
    const picked = issue(`i1`, `APP-1`)
    const model = fakeModel({
      eligibleIssues: [picked, issue(`i2`, `APP-2`)],
      checkedIssues: [picked],
      subject: { kind: `issues`, ids: [`i1`] },
    })
    render(<LaunchComposer model={model} users={[]} />)
    openPlus()
    fireEvent.click(screen.getByTestId(`agent-composer-menu-implement-issue`))
    const picker = screen.getByTestId(`agent-composer-issues-picker`)
    fireEvent.click(within(picker).getByText(`Issue APP-2`))
    expect(model.toggleIssue).toHaveBeenCalledWith(`i2`)
    const implement = screen.getByTestId(`agent-composer-implement`)
    expect(implement.textContent).toBe(`Implement 1 issue`)
    fireEvent.click(implement)
    expect(screen.queryByTestId(`agent-composer-menu`)).toBeNull()
  })

  it(`is the bottom sheet of the same rows on a phone`, () => {
    const width = window.innerWidth
    Object.defineProperty(window, `innerWidth`, { configurable: true, value: 390 })
    try {
      render(
        <LaunchComposer
          model={fakeModel({ eligibleIssues: [issue(`i1`, `APP-1`)] })}
          users={[]}
        />
      )
      fireEvent.click(screen.getByTestId(`agent-composer-plus-button`))
      const sheet = document.querySelector<HTMLElement>(`[data-slot=menu-sheet]`)!
      expect(sheet).toBeTruthy()
      fireEvent.click(within(sheet).getByTestId(`agent-composer-menu-implement-issue`))
      // The submenu is a pushed page: back row + the issue picker.
      expect(within(sheet).getByTestId(`menu-sheet-back`).textContent).toContain(
        `Implement issue`
      )
      expect(within(sheet).getByTestId(`agent-composer-issues-picker`)).toBeTruthy()
    } finally {
      Object.defineProperty(window, `innerWidth`, { configurable: true, value: width })
    }
  })

  it(`takes any file and shows a pending file as a named tile`, () => {
    const { container } = render(
      <LaunchComposer
        model={fakeModel({
          images: [
            {
              kind: `file`,
              file: new File([`x`], `notes.pdf`, { type: `application/pdf` }),
              url: `blob:f`,
            },
          ],
        })}
        users={[]}
      />
    )
    const input = container.querySelector(`input[type="file"]`)!
    expect(input.hasAttribute(`accept`)).toBe(false)
    expect(screen.getByRole(`button`, { name: `Remove notes.pdf` })).toBeTruthy()
    expect(
      container.querySelector(`[data-slot="attachment-file-tile"]`)!.textContent
    ).toBe(`notes.pdf`)
  })

  // F6: a picked device without the `steer-files` cap narrows the pick to images.
  it(`narrows the chooser to images for a device that takes no files`, () => {
    const { container } = render(
      <LaunchComposer model={fakeModel({ acceptsFiles: false })} users={[]} />
    )
    const input = container.querySelector(`input[type="file"]`)!
    expect(input.getAttribute(`accept`)).toBe(`image/*`)
  })

  it(`opens the file chooser from Add file or image`, () => {
    const click = vi.spyOn(HTMLInputElement.prototype, `click`)
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    openPlus()
    fireEvent.click(screen.getByTestId(`agent-composer-menu-add-file`))
    expect(click).toHaveBeenCalled()
    click.mockRestore()
  })
})

// EXP-980: the blocked-start dialog the composer opens on a blocked issue.
describe(`LaunchComposer blocked start`, () => {
  it(`offers Cancel, Start anyway and a disabled Stacked PR with its reason`, () => {
    const startAnyway = vi.fn().mockResolvedValue(undefined)
    render(
      <LaunchComposer
        model={fakeModel({
          subject: { kind: `issues`, ids: [`i1`] },
          checkedIssues: [issue(`i1`, `APP-1`)],
          blockedStart: [issue(`i2`, `APP-2`)],
          blockedOpen: true,
          blockedStack: { plan: null, reason: `running`, ident: `APP-2`, first: null },
          startAnyway,
          blocked: false,
        })}
        users={[]}
      />
    )
    expect(screen.getByText(BLOCKED_START_TITLE)).toBeTruthy()
    // The blocker rides an ordinary issue chip.
    expect(screen.getByTestId(`blocked-start-chip-APP-2`)).toBeTruthy()
    expect(screen.getByText(`Cancel`)).toBeTruthy()
    // EXP-1167: one flowing sentence, the "." glued to the last chip.
    const sentence = screen.getByTestId(`blocked-start-sentence`)
    expect(sentence.textContent?.endsWith(` Start anyway?`)).toBe(true)
    expect(
      screen.getByTestId(`blocked-start-chip-APP-2`).closest(`.whitespace-nowrap`)
        ?.textContent?.endsWith(`.`)
    ).toBe(true)
    const stacked = screen.getByTestId(`blocked-start-stacked`)
    expect(stacked.textContent).toBe(STACKED_PR_LABEL)
    expect((stacked as HTMLButtonElement).disabled).toBe(true)
    expect(screen.getByTestId(`blocked-start-stack-note`).textContent).toBe(
      `#APP-2 is already running. Its pull request is not open yet.`
    )
    expect(screen.queryByTestId(`blocked-start-plan-note`)).toBeNull()
    fireEvent.click(screen.getByText(START_ANYWAY_LABEL))
    expect(startAnyway).toHaveBeenCalledTimes(1)
  })

  it(`starts a stacked PR and says which issue of the line starts first`, () => {
    const startStacked = vi.fn().mockResolvedValue(undefined)
    const blocker = issue(`i2`, `APP-2`)
    render(
      <LaunchComposer
        model={fakeModel({
          subject: { kind: `issues`, ids: [`i1`] },
          checkedIssues: [issue(`i1`, `APP-1`)],
          blockedStart: [blocker],
          blockedOpen: true,
          blockedStack: {
            plan: { base: null, run: [`APP-2`, `APP-1`] },
            reason: null,
            ident: null,
            first: blocker,
          },
          startStacked,
          blocked: false,
        })}
        users={[]}
      />
    )
    // EXP-1167: one flowing sentence, the "." glued to the last chip.
    const sentence = screen.getByTestId(`blocked-start-sentence`)
    expect(sentence.textContent?.endsWith(` Start anyway, or start a stacked PR?`)).toBe(true)
    expect(
      screen.getByTestId(`blocked-start-chip-APP-2`).closest(`.whitespace-nowrap`)
        ?.textContent?.endsWith(`.`)
    ).toBe(true)
    expect(screen.queryByTestId(`blocked-start-stack-note`)).toBeNull()
    expect(screen.getByTestId(`blocked-start-plan-note`).textContent).toBe(
      `Starts #APP-2 first, then #APP-1.`
    )
    const stacked = screen.getByTestId(`blocked-start-stacked`)
    expect((stacked as HTMLButtonElement).disabled).toBe(false)
    fireEvent.click(stacked)
    expect(startStacked).toHaveBeenCalledTimes(1)
  })

  it(`asks about a batch with the batch copy`, () => {
    render(
      <LaunchComposer
        model={fakeModel({
          subject: { kind: `issues`, ids: [`i1`, `i3`] },
          checkedIssues: [issue(`i1`, `APP-1`), issue(`i3`, `APP-3`)],
          blockedStart: [issue(`i2`, `APP-2`)],
          blockedOpen: true,
          blocked: false,
        })}
        users={[]}
      />
    )
    expect(screen.getByText(BLOCKED_BATCH_TITLE)).toBeTruthy()
    expect(screen.getByText(BLOCKED_BATCH_BODY)).toBeTruthy()
  })

  it(`draws the transitive chain as the mini-graph`, () => {
    graphState.issues = [
      issue(`i1`, `APP-1`),
      issue(`i2`, `APP-2`),
      issue(`i4`, `APP-4`),
    ]
    graphState.relations = [
      { type: `blocks`, issueId: `i2`, relatedIssueId: `i1` },
      { type: `blocks`, issueId: `i4`, relatedIssueId: `i2` },
    ]
    render(
      <LaunchComposer
        model={fakeModel({
          subject: { kind: `issues`, ids: [`i1`] },
          checkedIssues: [issue(`i1`, `APP-1`)],
          blockedStart: [issue(`i2`, `APP-2`)],
          blockedOpen: true,
          blocked: false,
        })}
        users={[]}
      />
    )
    const wave = (identifier: string) =>
      screen
        .getByTestId(`issue-graph-node-${identifier}`)
        .getAttribute(`data-wave`)
    expect([wave(`APP-4`), wave(`APP-2`), wave(`APP-1`)]).toEqual([`0`, `1`, `2`])
    graphState.issues = []
    graphState.relations = []
  })

  it(`stays shut while nothing blocks the subject`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    expect(screen.queryByText(BLOCKED_START_TITLE)).toBeNull()
  })
})

import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue } from "@/db/schema"
import type { ComposerPrompt, LaunchComposerModel } from "@/hooks/use-launch-composer"
import type { LaunchOptions } from "@/components/launch-dialog/use-launch-options"
import type { SteerDevice } from "@/lib/steer-devices"

// EXP-825: the composer card over a FAKE model: chips, the per-subject
// submit label and placeholder, Enter-sends and the pending submit. The hook
// has its own tests; this only proves the chrome reads the model.

vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})
vi.mock(`@/lib/collections`, () => ({
  promptCollection: {},
  boardCollection: {},
  issueCollection: {},
  pullRequestCollection: {},
  pullRequestIssueCollection: {},
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

import { LaunchComposer } from "@/components/launch-composer"

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

const teamPrompt = (id: string, name: string) =>
  ({ id, teamId: `t1`, name, icon: null, sortOrder: 1 }) as unknown as ComposerPrompt

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
    selectedPrompt: null,
    prompts: [teamPrompt(`pr-1`, `Triage`), teamPrompt(`pr-2`, `Release train`)],
    eligibleIssues: [],
    checkedIssues: [],
    toggleIssue: vi.fn(),
    agents: [],
    selectedAgent: null,
    pickAgent: vi.fn(),
    clearAgent: vi.fn(),
    pickPrompt: vi.fn(),
    clearPrompt: vi.fn(),
    text: ``,
    setText: vi.fn(),
    images: [],
    addFiles: vi.fn(() => 0),
    removeImage: vi.fn(),
    repos: [],
    repoId: ``,
    setRepoId: vi.fn(),
    repoOptions: [],
    launch: fakeLaunch(),
    candidateDevices: [device],
    deviceRequestNote: null,
    mcpServers: null,
    mcpConnectHref: () => null,
    submitLabel: `Start session`,
    blocked: true,
    overCap: false,
    spansRepos: false,
    spansBranches: false,
    costHint: false,
    busy: false,
    submit: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  }
}

describe(`LaunchComposer`, () => {
  it(`renders the chat state: placeholder, an icon-only submit, no suggestions`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    expect(screen.getByTestId(`agent-composer`)).toBeTruthy()
    // The dialog keeps the contract's chat placeholder.
    expect(screen.getByPlaceholderText(`Ask the agent…`)).toBeTruthy()
    const submit = screen.getByTestId(`agent-composer-submit`) as HTMLButtonElement
    // EXP-827: the label is the button's NAME, never its text.
    expect(submit.getAttribute(`aria-label`)).toBe(`Start session`)
    expect(submit.textContent).toBe(``)
    expect(submit.disabled).toBe(true)
    // EXP-1270: the suggestion rows are gone.
    expect(screen.queryByTestId(`agent-composer-suggestion`)).toBeNull()
    expect(screen.queryByTestId(/agent-composer-chip-/)).toBeNull()
  })

  // EXP-1253: the home column asks composer-home.json's question; a subject
  // still turns the field into the optional half.
  it(`asks what to build on the home column`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} variant="home" />)
    expect(screen.getByPlaceholderText(`What do you want to build?`)).toBeTruthy()
    render(
      <LaunchComposer
        model={fakeModel({
          subject: { kind: `issues`, ids: [`i1`] },
          checkedIssues: [issue(`i1`, `APP-1`)],
        })}
        users={[]}
        variant="home"
      />
    )
    expect(screen.getByPlaceholderText(`Additional instructions (optional)…`)).toBeTruthy()
  })

  // EXP-1293: while the start uploads and sends, the submit goes pending
  // (after the 150 ms delay).
  it(`shows the pending submit while the session starts`, () => {
    vi.useFakeTimers()
    try {
      render(
        <LaunchComposer
          model={fakeModel({ busy: true, blocked: true })}
          users={[]}
          variant="home"
        />
      )
      const submit = screen.getByTestId(`agent-composer-submit`)
      expect(submit.getAttribute(`aria-busy`)).toBe(`true`)
      act(() => vi.advanceTimersByTime(150))
      expect(submit.getAttribute(`aria-label`)).toBe(`Starting`)
      expect(submit.querySelector(`[data-slot=spinner]`)).not.toBeNull()
      expect(screen.queryByTestId(`composer-starting`)).toBeNull()
    } finally {
      vi.useRealTimers()
    }
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

  it(`draws the prompt chip as the headline; only its ✕ clears it`, () => {
    const triage = teamPrompt(`pr-1`, `Triage`)
    const model = fakeModel({
      subject: { kind: `prompt`, id: triage.id },
      selectedPrompt: triage,
    })
    render(<LaunchComposer model={model} users={[]} />)
    expect(screen.getByTestId(`agent-composer-headline`).textContent).toBe(`Triage`)
    const chip = screen.getByTestId(`agent-composer-chip-prompt`)
    fireEvent.click(chip)
    expect(model.clearPrompt).not.toHaveBeenCalled()
    fireEvent.click(screen.getByTestId(`agent-composer-chip-prompt-remove`))
    expect(model.clearPrompt).toHaveBeenCalled()
    // The field is the optional half beside a prompt.
    expect(screen.getByPlaceholderText(/Additional instructions/)).toBeTruthy()
  })

  // NEW-2: the launch composer's `/` menu = the team's prompts ONLY (no
  // builtins before a session exists); a pick makes the prompt the subject
  // and clears the `/name` it was typed as.
  it(`lists only the team's prompts on /`, () => {
    render(<LaunchComposer model={fakeModel({ text: `/` })} users={[]} />)
    const menu = screen.getByTestId(`slash-menu`)
    expect(within(menu).getByText(`Triage`)).toBeTruthy()
    expect(within(menu).getByText(`Release train`)).toBeTruthy()
    expect(within(menu).queryByText(`/compact`)).toBeNull()
    expect(within(menu).queryByText(`/model`)).toBeNull()
  })

  it(`typing /name and Enter picks the prompt instead of sending`, () => {
    const model = fakeModel({ text: `/release`, blocked: false })
    render(<LaunchComposer model={model} users={[]} />)
    const menu = screen.getByTestId(`slash-menu`)
    expect(within(menu).queryByText(`Triage`)).toBeNull()
    fireEvent.keyDown(screen.getByTestId(`agent-composer-field`), { key: `Enter` })
    expect(model.pickPrompt).toHaveBeenCalledWith(`pr-2`)
    expect(model.setText).toHaveBeenCalledWith(``)
    expect(model.submit).not.toHaveBeenCalled()
  })

  it(`picks a prompt with a click`, () => {
    const model = fakeModel({ text: `/` })
    render(<LaunchComposer model={model} users={[]} />)
    fireEvent.click(within(screen.getByTestId(`slash-menu`)).getByText(`Triage`))
    expect(model.pickPrompt).toHaveBeenCalledWith(`pr-1`)
  })

  it(`opens no menu without prompts`, () => {
    render(<LaunchComposer model={fakeModel({ text: `/`, prompts: [] })} users={[]} />)
    expect(screen.queryByTestId(`slash-menu`)).toBeNull()
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

  // EXP-872: the options line — ONE account picker over the machine's
  // flattened logins (brand mark + email, never a profile name), the agent
  // implied by the pick; one login collapses to plain text.
  it(`says the one login as plain text`, () => {
    const model = fakeModel({
      launch: {
        ...fakeLaunch(),
        accountKey: `claude:home`,
        accountOptions: [
          { id: `home`, agent: `claude`, email: `me@example.com`, isLastUsed: true, health: `ok` },
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

  // EXP-1293: no Subagents row (`sessions.send` carries no subagent model).
  it(`lists the fixture's rows for claude without MCP servers or computer use`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    openPlus()
    expect(menuRowIds()).toEqual([
      `implement-issue`,
      `add-file`,
      `-`,
      `effort`,
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

  it(`says Reasoning and drops the claude-only rows for codex`, () => {
    const model = fakeModel({ launch: { ...fakeLaunch(), agent: `codex` } })
    render(<LaunchComposer model={model} users={[]} />)
    openPlus()
    expect(menuRowIds()).toEqual([`implement-issue`, `add-file`, `-`, `effort`])
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
    // The "+" row and the paperclip share the any-file chooser; the image
    // button's chooser filters to images.
    const input = screen.getByTestId(`composer-file-input`)
    expect(input.hasAttribute(`accept`)).toBe(false)
    expect(screen.getByTestId(`composer-image-input`).getAttribute(`accept`)).toContain(`image/`)
    expect(screen.getByRole(`button`, { name: `Remove notes.pdf` })).toBeTruthy()
    expect(
      container.querySelector(`[data-slot="attachment-file-tile"]`)!.textContent
    ).toBe(`notes.pdf`)
  })

  it(`opens the file chooser from Add file or image`, () => {
    const click = vi.spyOn(HTMLInputElement.prototype, `click`)
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    openPlus()
    fireEvent.click(screen.getByTestId(`agent-composer-menu-add-file`))
    expect(click).toHaveBeenCalled()
    expect(click.mock.contexts[0]).toBe(screen.getByTestId(`composer-file-input`))
    click.mockRestore()
  })

  it(`carries the inline edit tools after the "+" (EXP-1334: the one composer)`, () => {
    render(<LaunchComposer model={fakeModel()} users={[]} />)
    for (const name of [`Add image`, `Attach files`, `Insert issue reference`, `Insert emoji`]) {
      expect(screen.getByRole(`button`, { name })).toBeTruthy()
    }
    // The options line sits UNDER the card, inside the one frame.
    const frame = screen.getByTestId(`agent-composer`).closest(`[data-slot="composer-frame"]`)!
    expect(frame.contains(screen.getByTestId(`agent-options-row`))).toBe(true)
  })
})

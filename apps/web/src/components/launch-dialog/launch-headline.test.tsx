import { render, screen } from "@testing-library/react"
import { contract } from "@exp/domain-contract"
import { describe, expect, it, vi } from "vitest"

import type { Issue } from "@/db/schema"
import type { ComposerPrompt, LaunchComposerModel } from "@/hooks/use-launch-composer"

vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})
vi.mock(`@/lib/collections`, () => ({
  promptCollection: {},
  boardCollection: {},
  issueCollection: {},
}))

import {
  LaunchHeadline,
  launchHeadlineText,
  launchHeadlineVerb,
} from "@/components/launch-dialog/launch-headline"

// EXP-1019: the launcher's headline — the subject as the MAIN element, in the
// contract's verbs; NEW-2: a picked prompt's headline is its name.

const issue = (id: string, identifier: string): Issue =>
  ({
    id,
    identifier,
    title: `Issue ${identifier}`,
    status: `backlog`,
    priority: `none`,
    boardId: `b1`,
  }) as unknown as Issue

function model(overrides: Partial<LaunchComposerModel>): LaunchComposerModel {
  return {
    subject: null,
    selectedPrompt: null,
    checkedIssues: [],
    toggleIssue: vi.fn(),
    clearPrompt: vi.fn(),
    busy: false,
    ...overrides,
  } as unknown as LaunchComposerModel
}

describe(`launchHeadlineVerb`, () => {
  it(`is the contract's verb per subject`, () => {
    expect(launchHeadlineVerb(null)).toBe(contract.composerUi.chatHeadline)
    // A prompt needs no verb: its name IS the headline.
    expect(launchHeadlineVerb({ kind: `prompt`, id: `p1` })).toBe(``)
    expect(launchHeadlineVerb({ kind: `issues`, ids: [`i1`] })).toBe(
      contract.composerUi.implementHeadline
    )
  })
})

describe(`launchHeadlineText`, () => {
  it(`is the prompt's name`, () => {
    const triage = { id: `p-1`, name: `Triage`, icon: null } as unknown as ComposerPrompt
    expect(
      launchHeadlineText(
        model({ subject: { kind: `prompt`, id: triage.id }, selectedPrompt: triage })
      )
    ).toBe(`Triage`)
    // Not synced yet: the shape, never an empty title.
    expect(launchHeadlineText(model({ subject: { kind: `prompt`, id: `p-2` } }))).toBe(
      `Prompt`
    )
  })

  it(`lists the identifiers, and counts the rows that have not synced`, () => {
    expect(
      launchHeadlineText(
        model({
          subject: { kind: `issues`, ids: [`i1`, `i2`, `i3`] },
          checkedIssues: [issue(`i1`, `APP-3`), issue(`i2`, `APP-6`)],
        })
      )
    ).toBe(`Implement APP-3, APP-6, 1 more`)
  })

  it(`is the bare verb for a chat`, () => {
    expect(launchHeadlineText(model({}))).toBe(contract.composerUi.chatHeadline)
  })
})

describe(`LaunchHeadline`, () => {
  it(`renders nothing for a chat — there is no subject to announce`, () => {
    const { container } = render(<LaunchHeadline model={model({})} />)
    expect(container.firstChild).toBeNull()
  })

  it(`leads with the verb and carries the removable subject chips`, () => {
    render(
      <LaunchHeadline
        model={model({
          subject: { kind: `issues`, ids: [`i1`] },
          checkedIssues: [issue(`i1`, `APP-3`)],
        })}
      />
    )
    const headline = screen.getByTestId(`agent-composer-headline`)
    expect(headline.textContent).toContain(contract.composerUi.implementHeadline)
    // The chips moved out of the composer card and into the headline, ✕ and
    // all — the id the native suites match on is unchanged.
    expect(
      headline.querySelector(`[data-testid="agent-composer-chip-issue-APP-3"]`)
    ).toBeTruthy()
    expect(
      headline.querySelector(
        `[data-testid="agent-composer-chip-issue-APP-3-remove"]`
      )
    ).toBeTruthy()
  })

  it(`draws a picked prompt as its removable chip, with no verb`, () => {
    const clearPrompt = vi.fn()
    const triage = { id: `p-1`, name: `Triage`, icon: null } as unknown as ComposerPrompt
    render(
      <LaunchHeadline
        model={model({
          subject: { kind: `prompt`, id: triage.id },
          selectedPrompt: triage,
          clearPrompt,
        })}
      />
    )
    const headline = screen.getByTestId(`agent-composer-headline`)
    expect(headline.textContent).toBe(`Triage`)
    const chip = screen.getByTestId(`agent-composer-chip-prompt`)
    expect(chip.textContent).toContain(`Triage`)
    screen.getByTestId(`agent-composer-chip-prompt-remove`).click()
    expect(clearPrompt).toHaveBeenCalledTimes(1)
  })
})

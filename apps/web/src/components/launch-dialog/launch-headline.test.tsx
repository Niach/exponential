import { render, screen } from "@testing-library/react"
import { contract } from "@exp/domain-contract"
import { describe, expect, it, vi } from "vitest"

import type { Issue } from "@/db/schema"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import { builtinFixConflictsAction } from "@/lib/builtin-actions"

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

import {
  LaunchHeadline,
  launchHeadlineText,
  launchHeadlineVerb,
} from "@/components/launch-dialog/launch-headline"

// EXP-1019: the launcher's headline — the subject as the MAIN element, in the
// contract's two verbs.

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
    selectedAction: null,
    checkedIssues: [],
    fixConflicts: null,
    toggleIssue: vi.fn(),
    clearAction: vi.fn(),
    setInput: vi.fn(),
    busy: false,
    ...overrides,
  } as unknown as LaunchComposerModel
}

describe(`launchHeadlineVerb`, () => {
  it(`is the contract's verb per subject`, () => {
    expect(launchHeadlineVerb(null)).toBe(contract.composerUi.chatHeadline)
    expect(launchHeadlineVerb({ kind: `action`, id: `a1`, inputs: {} })).toBe(
      contract.composerUi.runHeadline
    )
    expect(launchHeadlineVerb({ kind: `issues`, ids: [`i1`] })).toBe(
      contract.composerUi.implementHeadline
    )
    // EXP-1233: the Fix merge conflicts builtin with a PICKED pull request.
    expect(
      launchHeadlineVerb({ kind: `action`, id: `a1`, inputs: {} }, true)
    ).toBe(contract.composerUi.fixConflictsHeadline)
  })
})

describe(`launchHeadlineText`, () => {
  it(`names the action`, () => {
    const fix = builtinFixConflictsAction(`t1`)
    expect(
      launchHeadlineText(
        model({
          subject: { kind: `action`, id: fix.id, inputs: {} },
          selectedAction: fix,
        })
      )
    ).toBe(`Run ${fix.name}`)
  })

  // EXP-1233: with a PR picked the verb is the builtin's own and the chips
  // are the PR's issues, so the accessible name reads like an issue start.
  it(`names the picked pull request's issues for Fix merge conflicts`, () => {
    const fix = builtinFixConflictsAction(`t1`)
    expect(
      launchHeadlineText(
        model({
          subject: { kind: `action`, id: fix.id, inputs: { pr: `i1` } },
          selectedAction: fix,
          fixConflicts: {
            refused: true,
            pr: {
              issueId: `i1`,
              prNumber: 9,
              branch: `exp/batch-1`,
              baseBranch: `master`,
              issues: [issue(`i1`, `APP-2`), issue(`i2`, `APP-3`)],
            },
          },
        })
      )
    ).toBe(`Fix merge conflicts APP-2, APP-3`)
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
})

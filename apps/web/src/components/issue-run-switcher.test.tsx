import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  IssueRunSwitcher,
  issueRunEntryLabel,
  issueRunWhen,
} from "@/components/issue-run-switcher"
import type { PastRunRow } from "@/hooks/use-agents-data"
import type { CodingSession } from "@/db/schema"

// EXP-886: the run switcher — absent under two runs, names the run on show,
// lists every run (live first) and opens the picked one.

function row(over: Partial<CodingSession> & { id: string }): PastRunRow {
  const session = {
    status: `ended`,
    userId: `me`,
    issueId: `issue-1`,
    deviceLabel: `macbook`,
    deviceId: `dev-1`,
    startedAt: new Date(`2026-09-01T09:00:00Z`),
    endedAt: new Date(`2026-09-01T10:00:00Z`),
    updatedAt: new Date(`2026-09-01T10:00:00Z`),
    ...over,
  } as CodingSession
  return {
    session,
    issue: undefined,
    batchIssues: [],
    board: undefined,
    device: { label: `macbook`, online: true },
    canResume: false,
    title: `Fix the sync loop`,
    identifier: `EXP-1`,
  }
}

function openMenu() {
  act(() => {
    fireEvent.pointerDown(screen.getByTestId(`issue-run-switcher`), {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

describe(`IssueRunSwitcher`, () => {
  it(`renders nothing under two runs`, () => {
    const { container } = render(
      <IssueRunSwitcher runs={[row({ id: `a` })]} viewedRunId="a" onOpen={vi.fn()} />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`names the run on show and lists every run, the live one marked`, () => {
    const live = row({
      id: `live`,
      status: `running`,
      endedAt: null,
      updatedAt: new Date(),
    })
    const ended = row({ id: `ended` })
    render(
      <IssueRunSwitcher runs={[live, ended]} viewedRunId="live" onOpen={vi.fn()} />
    )
    // The trigger carries the viewed run's `<when>` — `Live` here.
    expect(screen.getByTestId(`issue-run-switcher`).textContent).toBe(`Live`)
    openMenu()
    const options = document.querySelectorAll(`[data-testid^="issue-run-option-"]`)
    expect(options).toHaveLength(2)
    expect(options[0]?.textContent).toBe(`macbook · Live`)
    expect(options[1]?.textContent).toMatch(/^macbook · .+ago$/)
    // The run on show is the checked entry.
    expect(options[0]?.getAttribute(`data-state`)).toBe(`checked`)
    expect(options[1]?.getAttribute(`data-state`)).toBe(`unchecked`)
  })

  it(`opens the picked run and never re-opens the one on show`, () => {
    const onOpen = vi.fn()
    const a = row({ id: `a` })
    const b = row({ id: `b`, endedAt: new Date(`2026-08-01T10:00:00Z`) })
    render(<IssueRunSwitcher runs={[a, b]} viewedRunId="a" onOpen={onOpen} />)
    openMenu()
    act(() => {
      fireEvent.click(screen.getByTestId(`issue-run-option-a`))
    })
    expect(onOpen).not.toHaveBeenCalled()
    openMenu()
    act(() => {
      fireEvent.click(screen.getByTestId(`issue-run-option-b`))
    })
    expect(onOpen).toHaveBeenCalledWith(b.session)
  })
})

describe(`run entries (×4 strings)`, () => {
  it(`say Live for a live run and the ended time otherwise`, () => {
    const now = new Date()
    // A stamp that parses to nothing: the time segment simply drops.
    const noStamp = new Date(NaN)
    expect(issueRunWhen({ status: `running`, endedAt: null, updatedAt: now })).toBe(
      `Live`
    )
    expect(issueRunWhen({ status: `in_review`, endedAt: null, updatedAt: now })).toBe(
      `Live`
    )
    expect(issueRunWhen({ status: `ended`, endedAt: null, updatedAt: noStamp })).toBe(``)
    expect(
      issueRunEntryLabel({
        session: { status: `ended`, endedAt: null, updatedAt: noStamp, deviceLabel: null },
        device: { label: `macbook` },
      })
    ).toBe(`macbook`)
    // The device label falls back to the row's start-time snapshot.
    expect(
      issueRunEntryLabel({
        session: {
          status: `running`,
          endedAt: null,
          updatedAt: now,
          deviceLabel: `studio`,
        },
        device: { label: null },
      })
    ).toBe(`studio · Live`)
  })
})

import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  issueRunEntryLabel,
  issueRunWhen,
} from "@/components/issue-run-switcher"
import {
  ISSUE_FACE_LABEL,
  runFaceLabel,
  WorkFaceToggle,
} from "@/components/team/work-face-toggle"
import { LIVE_RUN_LABEL } from "@/lib/past-runs"
import type { PastRunRow } from "@/hooks/use-agents-data"
import type { CodingSession } from "@/db/schema"

// EXP-886 / EXP-950: the run menu behind the `Runs` segment's caret — no
// caret under two runs, a lone `Runs` item still shown for it, every run
// listed (live first) and the picked one opened.

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

function toggle(
  runs: PastRunRow[],
  onOpen: (session: CodingSession) => void,
  checkedRunId?: string,
  withIssue = true
) {
  return (
    <WorkFaceToggle
      face="run"
      runMenu={{ runs, checkedRunId, onOpen }}
      items={[
        ...(withIssue
          ? [{ face: `issue` as const, label: ISSUE_FACE_LABEL, onSelect: vi.fn() }]
          : []),
        { face: `run`, label: runFaceLabel(runs.length > 1), onSelect: vi.fn() },
      ]}
    />
  )
}

describe(`the Runs segment's run menu`, () => {
  it(`has no caret under two runs`, () => {
    render(toggle([row({ id: `a` })], vi.fn(), `a`))
    expect(screen.getByTestId(`work-face-toggle`)).toBeTruthy()
    expect(screen.queryByTestId(`issue-run-switcher`)).toBeNull()
  })

  it(`shows a lone Runs item when the issue has several runs`, () => {
    // One run, one face: no control at all.
    const { container, unmount } = render(
      toggle([row({ id: `a` })], vi.fn(), `a`, false)
    )
    expect(container.innerHTML).toBe(``)
    unmount()
    // Several runs: the toggle shows for its caret alone.
    render(toggle([row({ id: `a` }), row({ id: `b` })], vi.fn(), `a`, false))
    expect(screen.getByTestId(`work-face-toggle`).textContent).toBe(`Runs`)
    expect(screen.getByTestId(`issue-run-switcher`)).toBeTruthy()
  })

  it(`lists every run, the live one marked and the one on show checked`, () => {
    const live = row({
      id: `live`,
      status: `running`,
      endedAt: null,
      updatedAt: new Date(),
    })
    const ended = row({ id: `ended` })
    render(toggle([live, ended], vi.fn(), `live`))
    openMenu()
    const options = document.querySelectorAll(`[data-testid^="issue-run-option-"]`)
    expect(options).toHaveLength(2)
    expect(options[0]?.textContent).toBe(`macbook · Live`)
    expect(options[1]?.textContent).toMatch(/^macbook · .+ago$/)
    expect(options[0]?.getAttribute(`data-state`)).toBe(`checked`)
    expect(options[1]?.getAttribute(`data-state`)).toBe(`unchecked`)
  })

  it(`opens the picked run without selecting a face`, () => {
    const onOpen = vi.fn()
    const onRun = vi.fn()
    const a = row({ id: `a` })
    const b = row({ id: `b`, endedAt: new Date(`2026-08-01T10:00:00Z`) })
    render(
      <WorkFaceToggle
        face="issue"
        runMenu={{ runs: [a, b], checkedRunId: `a`, onOpen }}
        items={[
          { face: `issue`, label: ISSUE_FACE_LABEL, onSelect: vi.fn() },
          { face: `run`, label: runFaceLabel(true), onSelect: onRun },
        ]}
      />
    )
    openMenu()
    expect(onRun).not.toHaveBeenCalled()
    act(() => {
      fireEvent.click(screen.getByTestId(`issue-run-option-b`))
    })
    expect(onOpen).toHaveBeenCalledWith(b.session)
    expect(onRun).not.toHaveBeenCalled()
  })
})

describe(`run entries (×4 strings)`, () => {
  it(`say Live for a live run and the ended time otherwise`, () => {
    const now = new Date()
    // A stamp that parses to nothing: the time segment simply drops.
    const noStamp = new Date(NaN)
    expect(
      issueRunWhen({
        status: `running`,
        endedBy: null,
        endedAt: null,
        updatedAt: now,
      })
    ).toBe(`Live`)
    expect(
      issueRunWhen({
        status: `in_review`,
        endedBy: null,
        endedAt: null,
        updatedAt: now,
      })
    ).toBe(`Live`)
    expect(
      issueRunWhen({
        status: `ended`,
        endedBy: `agent`,
        endedAt: null,
        updatedAt: noStamp,
      })
    ).toBe(``)
    // EXP-888: the sweep's end is not an end — the run reads Live.
    expect(
      issueRunWhen({
        status: `ended`,
        endedBy: `stale`,
        endedAt: null,
        updatedAt: now,
      })
    ).toBe(LIVE_RUN_LABEL)
    expect(
      issueRunEntryLabel({
        session: {
          status: `ended`,
          endedBy: `agent`,
          endedAt: null,
          updatedAt: noStamp,
          deviceLabel: null,
        },
        device: { label: `macbook` },
      })
    ).toBe(`macbook`)
    // The device label falls back to the row's start-time snapshot.
    expect(
      issueRunEntryLabel({
        session: {
          status: `running`,
          endedBy: null,
          endedAt: null,
          updatedAt: now,
          deviceLabel: `studio`,
        },
        device: { label: null },
      })
    ).toBe(`studio · Live`)
  })
})

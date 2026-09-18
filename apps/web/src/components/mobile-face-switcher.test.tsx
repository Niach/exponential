import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { MobileFaceSwitcher } from "@/components/mobile-face-switcher"
import type { PastRunRow } from "@/hooks/use-agents-data"
import type { CodingSession } from "@/db/schema"

// EXP-893: the phone's face switcher — hidden with nowhere to go, a direct
// toggle wearing the destination's icon with one target, a menu above the
// circle with two or more (run rows, Changes with its counts, Start coding).

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
    fireEvent.pointerDown(screen.getByTestId(`mobile-face-switcher`), {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

describe(`MobileFaceSwitcher`, () => {
  it(`renders nothing with nowhere to go`, () => {
    const { container } = render(
      <MobileFaceSwitcher
        faces={[`issue`]}
        face="issue"
        hasChanges={false}
        onFace={vi.fn()}
      />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`toggles straight to the one other face`, () => {
    const onFace = vi.fn()
    render(
      <MobileFaceSwitcher
        faces={[`issue`, `run`]}
        face="issue"
        runs={[row({ id: `a`, status: `running` })]}
        viewedRunId="a"
        hasChanges={false}
        sessionTone="running"
        onFace={onFace}
      />
    )
    const circle = screen.getByTestId(`mobile-face-switcher`)
    expect(circle.getAttribute(`data-face-target`)).toBe(`run`)
    expect(circle.getAttribute(`aria-label`)).toBe(`Run`)
    // Off the Run face the badge is the session's state dot.
    expect(
      screen.getByTestId(`mobile-face-switcher-badge`).getAttribute(`data-tone`)
    ).toBe(`running`)
    fireEvent.click(circle)
    expect(onFace).toHaveBeenCalledWith(`run`)
  })

  it(`opens a menu of every target with two or more`, () => {
    const onFace = vi.fn()
    render(
      <MobileFaceSwitcher
        faces={[`issue`, `run`, `changes`]}
        face="run"
        runs={[row({ id: `a`, status: `running` })]}
        viewedRunId="a"
        diffStats={{ additions: 12, deletions: 3 }}
        hasChanges
        sessionTone="running"
        onFace={onFace}
      />
    )
    const circle = screen.getByTestId(`mobile-face-switcher`)
    expect(circle.getAttribute(`data-face-target`)).toBe(`menu`)
    // On the Run face the badge says changes wait behind the switcher.
    expect(
      screen.getByTestId(`mobile-face-switcher-badge`).getAttribute(`data-tone`)
    ).toBe(`changes`)
    openMenu()
    expect(screen.getByTestId(`mobile-face-switcher-menu`)).toBeTruthy()
    expect(screen.getByTestId(`mobile-face-option-issue`).textContent).toBe(
      `Issue`
    )
    const changes = screen.getByTestId(`mobile-face-option-changes`)
    expect(changes.textContent).toContain(`Changes`)
    expect(changes.textContent).toContain(`+12`)
    // EXP-895: U+2212 MINUS SIGN, `DiffCounts`'s own spelling.
    expect(changes.textContent).toContain(`\u22123`)
    // The shown face itself is never a target.
    expect(screen.queryByTestId(`mobile-face-option-run`)).toBeNull()
    fireEvent.click(changes)
    expect(onFace).toHaveBeenCalledWith(`changes`)
  })

  it(`lists one row per run once the issue has several`, () => {
    const onOpenRun = vi.fn()
    const live = row({
      id: `live`,
      status: `running`,
      endedAt: null,
      updatedAt: new Date(),
    })
    const ended = row({ id: `ended` })
    render(
      <MobileFaceSwitcher
        faces={[`issue`, `run`]}
        face="run"
        runs={[live, ended]}
        viewedRunId="live"
        hasChanges={false}
        onFace={vi.fn()}
        onOpenRun={onOpenRun}
      />
    )
    openMenu()
    const runRow = (id: string) =>
      document.querySelector(
        `[data-testid="mobile-face-switcher-menu"] [data-value="${id}"]`
      )
    expect(runRow(`live`)).toBeNull()
    const other = runRow(`ended`)!
    expect(other.textContent).toContain(`macbook`)
    fireEvent.click(other)
    expect(onOpenRun).toHaveBeenCalledWith(ended.session)
  })

  it(`offers start coding once the shown run ended for good`, () => {
    const onStart = vi.fn()
    render(
      <MobileFaceSwitcher
        faces={[`issue`, `run`]}
        face="run"
        runs={[row({ id: `a` })]}
        viewedRunId="a"
        hasChanges={false}
        offerStart
        onFace={vi.fn()}
        onStart={onStart}
      />
    )
    openMenu()
    const start = screen.getByTestId(`mobile-face-option-start`)
    expect(start.textContent).toBe(`Start coding`)
    fireEvent.click(start)
    expect(onStart).toHaveBeenCalled()
  })

  // EXP-931: inside the EXPANDED composer the switcher is the usage ring's
  // neighbour, not a 52px bar circle — same menu, smaller box.
  it(`wears the composer's chrome in the inline variant`, () => {
    const onFace = vi.fn()
    const props = {
      faces: [`issue`, `run`, `changes`] as const,
      face: `run` as const,
      hasChanges: true,
      onFace,
    }
    const circle = render(<MobileFaceSwitcher {...props} />)
    const bar = screen.getByTestId(`mobile-face-switcher`).className
    expect(bar).toContain(`size-[52px]`)
    circle.unmount()

    render(<MobileFaceSwitcher {...props} variant="inline" />)
    const inline = screen.getByTestId(`mobile-face-switcher`)
    expect(inline.className).not.toContain(`size-[52px]`)
    expect(inline.className).toContain(`size-6`)
    // Still the same menu behind it.
    openMenu()
    fireEvent.click(screen.getByTestId(`mobile-face-option-changes`))
    expect(onFace).toHaveBeenCalledWith(`changes`)
  })
})

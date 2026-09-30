import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  MobileFaceTabs,
  swipeDirection,
  useFaceSwipe,
} from "@/components/mobile-face-tabs"
import type { PastRunRow } from "@/hooks/use-agents-data"
import type { CodingSession } from "@/db/schema"
import type { WorkFaceKind } from "@/lib/work-faces"

// EXP-1150: the phone's faces are tabs — the strip under the header lists
// every available face, the `Runs` segment carries its caret with several
// runs, and a horizontal swipe on the body walks the strip.

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

function Swipeable({
  faces,
  face,
  onFace,
}: {
  faces: WorkFaceKind[]
  face: WorkFaceKind
  onFace: (face: WorkFaceKind) => void
}) {
  const swipe = useFaceSwipe(faces, face, onFace)
  return (
    <div data-testid="body" {...swipe}>
      <input data-testid="field" />
    </div>
  )
}

function swipe(target: HTMLElement, from: [number, number], to: [number, number]) {
  fireEvent.touchStart(target, {
    touches: [{ clientX: from[0], clientY: from[1] }],
  })
  fireEvent.touchEnd(target, {
    changedTouches: [{ clientX: to[0], clientY: to[1] }],
  })
}

describe(`MobileFaceTabs`, () => {
  it(`renders nothing with a single face`, () => {
    const { container } = render(
      <MobileFaceTabs faces={[`issue`]} face="issue" onFace={vi.fn()} />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`lists every face in order and selects on tap`, () => {
    const onFace = vi.fn()
    render(
      <MobileFaceTabs
        faces={[`issue`, `run`, `changes`, `results`]}
        face="run"
        onFace={onFace}
      />
    )
    const toggle = screen.getByTestId(`work-face-toggle`)
    expect(
      Array.from(toggle.querySelectorAll(`[data-face]`)).map((node) =>
        node.textContent?.trim()
      )
    ).toEqual([`Issue`, `Run`, `Changes`, `Results`])
    expect(
      toggle.querySelector(`[data-face="run"]`)?.getAttribute(`data-state`)
    ).toBe(`active`)
    fireEvent.mouseDown(toggle.querySelector(`[data-face="diff"]`)!, {
      button: 0,
    })
    expect(onFace).toHaveBeenCalledWith(`changes`)
  })

  it(`reads Runs with a caret once the issue has several runs`, () => {
    render(
      <MobileFaceTabs
        faces={[`issue`, `run`]}
        face="issue"
        runs={[row({ id: `a` }), row({ id: `b` })]}
        viewedRunId="a"
        onFace={vi.fn()}
        onOpenRun={vi.fn()}
      />
    )
    expect(screen.getByText(`Runs`)).toBeTruthy()
    expect(screen.getByTestId(`issue-run-switcher`)).toBeTruthy()
  })
})

describe(`useFaceSwipe`, () => {
  it(`gates on distance and horizontal dominance`, () => {
    expect(swipeDirection(-80, 10)).toBe(`left`)
    expect(swipeDirection(80, -20)).toBe(`right`)
    expect(swipeDirection(-40, 0)).toBeNull()
    expect(swipeDirection(-80, 50)).toBeNull()
  })

  it(`walks the strip on a swipe of the body`, () => {
    const onFace = vi.fn()
    render(
      <Swipeable faces={[`issue`, `run`, `changes`]} face="run" onFace={onFace} />
    )
    const body = screen.getByTestId(`body`)
    swipe(body, [200, 100], [100, 110])
    expect(onFace).toHaveBeenLastCalledWith(`changes`)
    swipe(body, [100, 100], [220, 90])
    expect(onFace).toHaveBeenLastCalledWith(`issue`)
    // A vertical scroll is not a swipe.
    onFace.mockClear()
    swipe(body, [100, 100], [160, 300])
    expect(onFace).not.toHaveBeenCalled()
  })

  it(`leaves a touch that starts in a text field alone`, () => {
    const onFace = vi.fn()
    render(<Swipeable faces={[`issue`, `run`]} face="issue" onFace={onFace} />)
    const field = screen.getByTestId(`field`)
    swipe(field, [200, 100], [100, 100])
    expect(onFace).not.toHaveBeenCalled()
  })
})

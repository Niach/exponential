import { fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  MobileFaceTabs,
  releaseDirection,
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

describe(`MobileFaceTabs counts (EXP-1152)`, () => {
  it(`the Changes segment wears +N −M once the files are known`, () => {
    render(
      <MobileFaceTabs
        faces={[`issue`, `changes`]}
        face="issue"
        changesCounts={{ additions: 12, deletions: 2 }}
        onFace={vi.fn()}
      />
    )
    const segment = screen
      .getByTestId(`work-face-toggle`)
      .querySelector(`[data-face="diff"]`)
    expect(segment?.textContent).toBe(`+12 \u22122`)
    expect(segment?.querySelector(`[aria-label="+12 \u22122"]`)).not.toBeNull()
    expect(segment?.textContent).not.toContain(`Changes`)
  })

  it(`and the word until then`, () => {
    render(
      <MobileFaceTabs faces={[`issue`, `changes`]} face="issue" onFace={vi.fn()} />
    )
    const segment = screen
      .getByTestId(`work-face-toggle`)
      .querySelector(`[data-face="diff"]`)
    expect(segment?.textContent).toBe(`Changes`)
  })
})

// EXP-1152: the pager — the face's BODY follows the finger, rubber-bands at
// either end, and a commit waits for the exit transition before the face
// flips.
function Paged({
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
    <div data-testid="root" {...swipe}>
      <div data-testid="header">tabs</div>
      <div data-testid="body" data-face-body="">
        <div data-testid="content">text</div>
        <div data-testid="scroller" style={{ overflowX: `auto` }}>
          <span data-testid="code">const x = 1</span>
        </div>
        <textarea data-testid="field" />
      </div>
    </div>
  )
}

function drag(
  target: HTMLElement,
  points: [number, number][],
  lift = true
) {
  const [first, ...rest] = points
  fireEvent.touchStart(target, {
    touches: [{ clientX: first[0], clientY: first[1] }],
  })
  for (const [x, y] of rest) {
    fireEvent.touchMove(target, { touches: [{ clientX: x, clientY: y }] })
  }
  if (!lift) return
  const last = points[points.length - 1]
  fireEvent.touchEnd(target, {
    changedTouches: [{ clientX: last[0], clientY: last[1] }],
  })
}

function mockReducedMotion(reduce: boolean) {
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: reduce && query.includes(`reduce`),
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })) as unknown as typeof window.matchMedia
}

describe(`useFaceSwipe pager (EXP-1152)`, () => {
  beforeEach(() => {
    mockReducedMotion(false)
    // The enter direction is module state that outlives a face (the Run
    // face is another route): a mount consumes whatever an earlier test's
    // commit left behind.
    render(<Paged faces={[`issue`]} face="issue" onFace={vi.fn()} />).unmount()
  })
  afterEach(() => {
    // @ts-expect-error — jsdom has no matchMedia; put it back that way.
    delete window.matchMedia
  })

  it(`the body follows a horizontal drag; the root never moves`, () => {
    render(<Paged faces={[`issue`, `changes`]} face="issue" onFace={vi.fn()} />)
    const body = screen.getByTestId(`body`)
    drag(screen.getByTestId(`content`), [[200, 100], [190, 101], [150, 102]], false)
    expect(body.style.transform).toBe(`translateX(-50px)`)
    expect(screen.getByTestId(`root`).style.transform).toBe(``)
    expect(screen.getByTestId(`root`).style.touchAction).toBe(`pan-y pinch-zoom`)
  })

  it(`a vertical-dominant move leaves the body alone`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`issue`, `changes`]} face="issue" onFace={onFace} />)
    const body = screen.getByTestId(`body`)
    drag(screen.getByTestId(`content`), [[200, 100], [198, 120], [150, 220]])
    expect(body.style.transform).toBe(``)
    expect(onFace).not.toHaveBeenCalled()
  })

  it(`rubber-bands with no neighbour, then springs back`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`issue`, `changes`]} face="issue" onFace={onFace} />)
    const body = screen.getByTestId(`body`)
    drag(screen.getByTestId(`content`), [[100, 100], [110, 100], [200, 100]], false)
    expect(body.style.transform).toBe(`translateX(30px)`)
    fireEvent.touchEnd(screen.getByTestId(`content`), {
      changedTouches: [{ clientX: 200, clientY: 100 }],
    })
    expect(onFace).not.toHaveBeenCalled()
    expect(body.style.transform).toBe(`translateX(0px)`)
    fireEvent.transitionEnd(body)
    expect(body.style.transform).toBe(``)
  })

  it(`a commit leaves to the side, then flips the face after the transition`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`issue`, `changes`]} face="issue" onFace={onFace} />)
    const body = screen.getByTestId(`body`)
    drag(screen.getByTestId(`content`), [[300, 100], [280, 100], [200, 102]])
    expect(body.style.transform).toBe(`translateX(-100%)`)
    expect(onFace).not.toHaveBeenCalled()
    fireEvent.transitionEnd(body)
    expect(onFace).toHaveBeenCalledWith(`changes`)
  })

  it(`the new face's body slides in from the side the old one left by`, () => {
    const onFace = vi.fn()
    const { rerender } = render(
      <Paged faces={[`issue`, `changes`]} face="issue" onFace={onFace} />
    )
    drag(screen.getByTestId(`content`), [[300, 100], [280, 100], [200, 102]])
    fireEvent.transitionEnd(screen.getByTestId(`body`))
    rerender(<Paged faces={[`issue`, `changes`]} face="changes" onFace={onFace} />)
    const body = screen.getByTestId(`body`)
    // Entered from +100%, now settling at rest.
    expect(body.style.transform).toBe(`translateX(0px)`)
    expect(body.style.transition).toContain(`200ms`)
    fireEvent.transitionEnd(body)
    expect(body.style.transform).toBe(``)
  })

  it(`a quick flick commits under the distance gate`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`issue`, `changes`]} face="changes" onFace={onFace} />)
    drag(screen.getByTestId(`content`), [[100, 100], [110, 100], [130, 101]])
    fireEvent.transitionEnd(screen.getByTestId(`body`))
    expect(onFace).toHaveBeenCalledWith(`issue`)
    expect(releaseDirection(30, 2, 0.8)).toBe(`right`)
    expect(releaseDirection(30, 2, 0.2)).toBeNull()
    expect(releaseDirection(30, 2, -0.8)).toBeNull()
    expect(releaseDirection(20, 0, 2)).toBeNull()
    expect(releaseDirection(-30, 20, -2)).toBeNull()
  })

  it(`a text field keeps the gesture`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`issue`, `changes`]} face="issue" onFace={onFace} />)
    drag(screen.getByTestId(`field`), [[300, 100], [280, 100], [150, 100]])
    expect(screen.getByTestId(`body`).style.transform).toBe(``)
    expect(onFace).not.toHaveBeenCalled()
  })

  it(`a sideways scroller keeps the swipe while it can still scroll that way`, () => {
    const onFace = vi.fn()
    render(
      <Paged faces={[`issue`, `changes`, `results`]} face="changes" onFace={onFace} />
    )
    const scroller = screen.getByTestId(`scroller`)
    Object.defineProperty(scroller, `scrollWidth`, { value: 500 })
    Object.defineProperty(scroller, `clientWidth`, { value: 100 })
    scroller.scrollLeft = 0
    const code = screen.getByTestId(`code`)
    // Leftwards: the line still has 400px to go — the line scrolls.
    drag(code, [[300, 100], [280, 100], [150, 100]])
    expect(screen.getByTestId(`body`).style.transform).toBe(``)
    expect(onFace).not.toHaveBeenCalled()
    // Rightwards from its left edge: nothing to scroll — the page turns.
    drag(code, [[100, 100], [120, 100], [250, 100]])
    fireEvent.transitionEnd(screen.getByTestId(`body`))
    expect(onFace).toHaveBeenCalledWith(`issue`)
  })

  it(`a second finger cancels`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`issue`, `changes`]} face="issue" onFace={onFace} />)
    const content = screen.getByTestId(`content`)
    drag(content, [[300, 100], [280, 100], [200, 100]], false)
    fireEvent.touchMove(content, {
      touches: [
        { clientX: 190, clientY: 100 },
        { clientX: 250, clientY: 300 },
      ],
    })
    fireEvent.touchEnd(content, {
      changedTouches: [{ clientX: 100, clientY: 100 }],
    })
    expect(onFace).not.toHaveBeenCalled()
  })

  it(`reduced motion: no transform, the face flips at once`, () => {
    mockReducedMotion(true)
    const onFace = vi.fn()
    render(<Paged faces={[`issue`, `changes`]} face="issue" onFace={onFace} />)
    drag(screen.getByTestId(`content`), [[300, 100], [280, 100], [200, 100]])
    expect(screen.getByTestId(`body`).style.transform).toBe(``)
    expect(onFace).toHaveBeenCalledWith(`changes`)
  })
})

import { fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  releaseDirection,
  swipeDirection,
  swipeTarget,
  useFaceSwipe,
} from "@/hooks/use-face-swipe"

// EXP-1150/1152/1190: the phone pager every tab strip swipes with — a
// horizontal swipe on the body walks the strip, the body follows the finger
// and rubber-bands at either end, and a commit waits for the exit transition.

type Tab = `inbox` | `issues` | `drafts`

function Swipeable({
  faces,
  face,
  onFace,
}: {
  faces: Tab[]
  face: Tab
  onFace: (face: Tab) => void
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

describe(`swipeTarget`, () => {
  it(`is the neighbour in the strip, null at either end`, () => {
    const tabs: Tab[] = [`inbox`, `issues`, `drafts`]
    expect(swipeTarget(tabs, `issues`, `left`)).toBe(`drafts`)
    expect(swipeTarget(tabs, `issues`, `right`)).toBe(`inbox`)
    expect(swipeTarget(tabs, `drafts`, `left`)).toBeNull()
    expect(swipeTarget(tabs, `inbox`, `right`)).toBeNull()
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
    render(<Swipeable faces={[`inbox`, `issues`, `drafts`]} face="issues" onFace={onFace} />)
    const body = screen.getByTestId(`body`)
    swipe(body, [200, 100], [100, 110])
    expect(onFace).toHaveBeenLastCalledWith(`drafts`)
    swipe(body, [100, 100], [220, 90])
    expect(onFace).toHaveBeenLastCalledWith(`inbox`)
    // A vertical scroll is not a swipe.
    onFace.mockClear()
    swipe(body, [100, 100], [160, 300])
    expect(onFace).not.toHaveBeenCalled()
  })

  it(`leaves a touch that starts in a text field alone`, () => {
    const onFace = vi.fn()
    render(<Swipeable faces={[`inbox`, `issues`]} face="inbox" onFace={onFace} />)
    swipe(screen.getByTestId(`field`), [200, 100], [100, 100])
    expect(onFace).not.toHaveBeenCalled()
  })
})

function Paged({
  faces,
  face,
  onFace,
}: {
  faces: Tab[]
  face: Tab
  onFace: (face: Tab) => void
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
    render(<Paged faces={[`inbox`]} face="inbox" onFace={vi.fn()} />).unmount()
  })
  afterEach(() => {
    // @ts-expect-error — jsdom has no matchMedia; put it back that way.
    delete window.matchMedia
  })

  it(`the body follows a horizontal drag; the root never moves`, () => {
    render(<Paged faces={[`inbox`, `drafts`]} face="inbox" onFace={vi.fn()} />)
    const body = screen.getByTestId(`body`)
    drag(screen.getByTestId(`content`), [[200, 100], [190, 101], [150, 102]], false)
    expect(body.style.transform).toBe(`translateX(-50px)`)
    expect(screen.getByTestId(`root`).style.transform).toBe(``)
    expect(screen.getByTestId(`root`).style.touchAction).toBe(`pan-y pinch-zoom`)
  })

  it(`a vertical-dominant move leaves the body alone`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`inbox`, `drafts`]} face="inbox" onFace={onFace} />)
    const body = screen.getByTestId(`body`)
    drag(screen.getByTestId(`content`), [[200, 100], [198, 120], [150, 220]])
    expect(body.style.transform).toBe(``)
    expect(onFace).not.toHaveBeenCalled()
  })

  it(`rubber-bands with no neighbour, then springs back`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`inbox`, `drafts`]} face="inbox" onFace={onFace} />)
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
    render(<Paged faces={[`inbox`, `drafts`]} face="inbox" onFace={onFace} />)
    const body = screen.getByTestId(`body`)
    drag(screen.getByTestId(`content`), [[300, 100], [280, 100], [200, 102]])
    expect(body.style.transform).toBe(`translateX(-100%)`)
    expect(onFace).not.toHaveBeenCalled()
    fireEvent.transitionEnd(body)
    expect(onFace).toHaveBeenCalledWith(`drafts`)
  })

  it(`the new face's body slides in from the side the old one left by`, () => {
    const onFace = vi.fn()
    const { rerender } = render(
      <Paged faces={[`inbox`, `drafts`]} face="inbox" onFace={onFace} />
    )
    drag(screen.getByTestId(`content`), [[300, 100], [280, 100], [200, 102]])
    fireEvent.transitionEnd(screen.getByTestId(`body`))
    rerender(<Paged faces={[`inbox`, `drafts`]} face="drafts" onFace={onFace} />)
    const body = screen.getByTestId(`body`)
    // Entered from +100%, now settling at rest.
    expect(body.style.transform).toBe(`translateX(0px)`)
    expect(body.style.transition).toContain(`200ms`)
    fireEvent.transitionEnd(body)
    expect(body.style.transform).toBe(``)
  })

  it(`a quick flick commits under the distance gate`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`inbox`, `drafts`]} face="drafts" onFace={onFace} />)
    drag(screen.getByTestId(`content`), [[100, 100], [110, 100], [130, 101]])
    fireEvent.transitionEnd(screen.getByTestId(`body`))
    expect(onFace).toHaveBeenCalledWith(`inbox`)
    expect(releaseDirection(30, 2, 0.8)).toBe(`right`)
    expect(releaseDirection(30, 2, 0.2)).toBeNull()
    expect(releaseDirection(30, 2, -0.8)).toBeNull()
    expect(releaseDirection(20, 0, 2)).toBeNull()
    expect(releaseDirection(-30, 20, -2)).toBeNull()
  })

  it(`a text field keeps the gesture`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`inbox`, `drafts`]} face="inbox" onFace={onFace} />)
    drag(screen.getByTestId(`field`), [[300, 100], [280, 100], [150, 100]])
    expect(screen.getByTestId(`body`).style.transform).toBe(``)
    expect(onFace).not.toHaveBeenCalled()
  })

  it(`a sideways scroller keeps the swipe while it can still scroll that way`, () => {
    const onFace = vi.fn()
    render(
      <Paged faces={[`inbox`, `issues`, `drafts`]} face="issues" onFace={onFace} />
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
    expect(onFace).toHaveBeenCalledWith(`inbox`)
  })

  it(`a second finger cancels`, () => {
    const onFace = vi.fn()
    render(<Paged faces={[`inbox`, `drafts`]} face="inbox" onFace={onFace} />)
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
    render(<Paged faces={[`inbox`, `drafts`]} face="inbox" onFace={onFace} />)
    drag(screen.getByTestId(`content`), [[300, 100], [280, 100], [200, 100]])
    expect(screen.getByTestId(`body`).style.transform).toBe(``)
    expect(onFace).toHaveBeenCalledWith(`drafts`)
  })
})

// EXP-877: the unified work header's face toggle — byte-identical labels
// with the IDE (`work_header.rs`), hidden faces rather than disabled ones,
// and no control at all under two faces. EXP-1251: Issue · Run · Guide.

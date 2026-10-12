import { act, fireEvent, render, renderHook, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { FEEDBACK_STATES, markStripeSpan } from "@exp/domain-contract/feedback-mark"

import { Button } from "./button"
import {
  actionErrorTitle,
  BlockingOverlay,
  BlockingSurface,
  FieldError,
  blockingLabel,
  blockWindow,
  FeedbackMark,
  InlineStatus,
  markStripePath,
  Spinner,
  useDelayedFlag,
} from "./feedback"
import { SkeletonRows } from "./skeleton"

// EXP-1275 (EXP-1259 D8): the web side of feedback-states.json.

const DELAY = FEEDBACK_STATES.pending.delayMs

function mockReducedMotion(matches: boolean) {
  vi.stubGlobal(`matchMedia`, (query: string) => ({
    matches: matches && query.includes(`reduce`),
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }))
}

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe(`Spinner`, () => {
  it(`is the 10px inline glyph by default`, () => {
    const { container } = render(<Spinner />)
    const glyph = container.querySelector(`[data-slot=spinner]`)!
    expect(glyph.getAttribute(`width`)).toBe(String(FEEDBACK_STATES.inline.spinner))
    expect(glyph.getAttribute(`class`)).toContain(`animate-spin`)
    expect(glyph.getAttribute(`data-testid`)).toBe(FEEDBACK_STATES.testIds.spinner)
  })
})

describe(`useDelayedFlag`, () => {
  it(`turns true only after the delay and false at once`, () => {
    vi.useFakeTimers()
    const { result, rerender } = renderHook(({ flag }) => useDelayedFlag(flag), {
      initialProps: { flag: false },
    })
    rerender({ flag: true })
    expect(result.current).toBe(false)
    act(() => vi.advanceTimersByTime(DELAY - 1))
    expect(result.current).toBe(false)
    act(() => vi.advanceTimersByTime(1))
    expect(result.current).toBe(true)
    rerender({ flag: false })
    expect(result.current).toBe(false)
  })
})

describe(`InlineStatus`, () => {
  it(`saving = spinner + word, saved = word, idle = the settled caption`, () => {
    const { container, rerender } = render(<InlineStatus state="saving" />)
    expect(container.textContent).toBe(FEEDBACK_STATES.autosave.captions.saving)
    expect(container.querySelector(`[data-slot=spinner]`)).not.toBeNull()
    rerender(<InlineStatus state="saved" />)
    expect(container.textContent).toBe(FEEDBACK_STATES.autosave.captions.saved)
    expect(container.querySelector(`[data-slot=spinner]`)).toBeNull()
    rerender(<InlineStatus state="idle" savedLabel="Edited 2m ago" />)
    expect(container.textContent).toBe(`Edited 2m ago`)
    rerender(<InlineStatus state="idle" />)
    expect(container.querySelector(`[data-slot=inline-status]`)).toBeNull()
  })
})

describe(`Button pending`, () => {
  it(`shows the spinner and the progressive word only after the delay`, () => {
    vi.useFakeTimers()
    const onClick = vi.fn()
    const { rerender } = render(
      <Button onClick={onClick}>
        <svg data-testid="icon" />
        Start
      </Button>
    )
    rerender(
      <Button onClick={onClick} pending pendingLabel="Starting">
        <svg data-testid="icon" />
        Start
      </Button>
    )
    const button = screen.getByRole(`button`)
    // Inside the delay nothing shows, but a second click is refused.
    expect(button.textContent).toBe(`Start`)
    expect(button.getAttribute(`aria-busy`)).toBe(`true`)
    fireEvent.click(button)
    expect(onClick).not.toHaveBeenCalled()
    act(() => vi.advanceTimersByTime(DELAY))
    expect(button.textContent).toBe(`Starting`)
    expect(button.querySelector(`[data-slot=spinner]`)).not.toBeNull()
    expect(screen.queryByTestId(`icon`)).toBeNull()
    expect(button.hasAttribute(`disabled`)).toBe(true)
    expect(button.getAttribute(`data-testid`)).toBe(FEEDBACK_STATES.testIds.pending)
  })

  it(`a zero delay shows the pending state on the first render`, () => {
    render(
      <Button pending pendingLabel="Starting" pendingDelayMs={0}>
        Start
      </Button>
    )
    expect(screen.getByRole(`button`).textContent).toBe(`Starting`)
  })

  it(`without a word the icon slot alone gives way`, () => {
    vi.useFakeTimers()
    render(
      <Button pending>
        <svg data-testid="icon" />
        Connect
      </Button>
    )
    act(() => vi.advanceTimersByTime(DELAY))
    const button = screen.getByRole(`button`)
    expect(button.textContent).toBe(`Connect`)
    expect(screen.queryByTestId(`icon`)).toBeNull()
  })

  it(`an icon button turns into the bare spinner`, () => {
    vi.useFakeTimers()
    render(
      <Button size="icon" aria-label="Send" pending pendingLabel="Sending">
        <svg data-testid="icon" />
      </Button>
    )
    act(() => vi.advanceTimersByTime(DELAY))
    const button = screen.getByRole(`button`, { name: `Send` })
    expect(button.textContent).toBe(``)
    expect(button.querySelector(`[data-slot=spinner]`)).not.toBeNull()
  })
})

describe(`SkeletonRows`, () => {
  it(`draws rows in the asked geometry with a dot each`, () => {
    const { container } = render(<SkeletonRows rows={4} rowHeight={36} />)
    const block = container.querySelector(`[data-slot=skeleton-rows]`)!
    expect(block.getAttribute(`data-testid`)).toBe(FEEDBACK_STATES.testIds.skeleton)
    const rows = block.querySelectorAll(`[data-part=row]`)
    expect(rows).toHaveLength(4)
    expect((rows[0] as HTMLElement).style.height).toBe(`36px`)
    expect(block.querySelectorAll(`[data-part=dot]`)).toHaveLength(4)
  })

  it(`no dot when asked, fixture rows by default, static under reduce motion`, () => {
    mockReducedMotion(true)
    const { container } = render(<SkeletonRows dot={false} />)
    const block = container.querySelector(`[data-slot=skeleton-rows]`) as HTMLElement
    expect(block.querySelectorAll(`[data-part=row]`)).toHaveLength(FEEDBACK_STATES.placeholder.rows)
    expect(block.querySelector(`[data-part=dot]`)).toBeNull()
    expect(block.style.opacity).toBe(String(FEEDBACK_STATES.placeholder.reducedOpacity))
  })
})

describe(`FeedbackMark`, () => {
  it(`samples a span of the stripe's cubic and draws nothing for an empty one`, () => {
    expect(markStripePath(0, null)).toBe(``)
    expect(markStripePath(0, [0.5, 0.5])).toBe(``)
    const path = markStripePath(0, [0, 1])
    expect(path.startsWith(`M -5.87 62.01`)).toBe(true)
    expect(path.endsWith(`49.03 -6.21`)).toBe(true)
    expect(path.split(` L `)).toHaveLength(FEEDBACK_STATES.mark.samples + 1)
  })

  it(`starts on the plain disc and cuts at the fixture's stripe width`, () => {
    mockReducedMotion(false)
    const { container } = render(<FeedbackMark size={40} />)
    const svg = container.querySelector(`[data-slot=feedback-mark]`)!
    expect(svg.getAttribute(`width`)).toBe(`40`)
    const paths = [...svg.querySelectorAll(`mask path`)]
    expect(paths).toHaveLength(3)
    expect(paths.map((p) => p.getAttribute(`d`))).toEqual(
      [0, 1, 2].map((index) => markStripePath(index, markStripeSpan(index, 0)))
    )
    expect(paths[0]!.getAttribute(`stroke-width`)).toBe(String(FEEDBACK_STATES.mark.stripeWidth))
  })

  it(`freezes one frame of the loop at a given phase`, () => {
    mockReducedMotion(false)
    const { container } = render(<FeedbackMark phase={0.3} />)
    const paths = [...container.querySelectorAll(`mask path`)]
    expect(paths.map((p) => p.getAttribute(`d`))).toEqual(
      [0, 1, 2].map((index) => markStripePath(index, markStripeSpan(index, 0.3)))
    )
  })

  it(`is the static logo under reduce motion`, () => {
    mockReducedMotion(true)
    const { container } = render(<FeedbackMark />)
    const svg = container.querySelector(`[data-slot=feedback-mark]`)!
    expect(svg.getAttribute(`data-static`)).toBe(`true`)
    const paths = [...svg.querySelectorAll(`mask path`)]
    expect(paths.map((p) => p.getAttribute(`d`))).toEqual(
      [0, 1, 2].map((index) => markStripePath(index, [0, 1]))
    )
  })
})

describe(`BlockingOverlay`, () => {
  it(`covers the window with the mark and one line, only while open`, () => {
    const { rerender } = render(<BlockingOverlay open label="Signing out" />)
    const overlay = screen.getByTestId(FEEDBACK_STATES.testIds.blocking)
    expect(overlay.parentElement).toBe(document.body)
    expect(overlay.className).toContain(`fixed inset-0`)
    expect(overlay.textContent).toBe(`Signing out`)
    expect(overlay.querySelector(`[data-slot=feedback-mark]`)!.getAttribute(`width`)).toBe(
      String(FEEDBACK_STATES.blocking.markSize)
    )
    rerender(<BlockingOverlay open={false} label="Signing out" />)
    expect(screen.queryByTestId(FEEDBACK_STATES.testIds.blocking)).toBeNull()
  })

  it(`a contained surface fills its parent instead of the window`, () => {
    render(<BlockingSurface contained label="Switching to Personal" />)
    const surface = screen.getByTestId(FEEDBACK_STATES.testIds.blocking)
    expect(surface.className).toContain(`absolute inset-0`)
    expect(surface.className).not.toContain(`fixed`)
  })

  it(`blockWindow shows after the delay and releases idempotently`, async () => {
    vi.useFakeTimers()
    const release = blockWindow(blockingLabel(`switchTeam`, `Personal`))
    expect(document.querySelector(`[data-slot=blocking-host]`)).toBeNull()
    await act(async () => vi.advanceTimersByTime(DELAY))
    expect(document.body.textContent).toContain(`Switching to Personal`)
    act(() => release())
    release()
    expect(document.querySelector(`[data-slot=blocking-host]`)).toBeNull()
  })

  it(`a release inside the delay never shows anything`, () => {
    vi.useFakeTimers()
    const release = blockWindow(blockingLabel(`signOut`))
    release()
    vi.advanceTimersByTime(DELAY * 2)
    expect(document.querySelector(`[data-slot=blocking-host]`)).toBeNull()
  })
})

// FEED-79: the error rule's two halves, feedback-states.json `errors`.
describe(`errors`, () => {
  it(`titles a failed action per the pattern`, () => {
    for (const c of FEEDBACK_STATES.errors.cases.filter((c) => c.kind === `action`)) {
      expect(actionErrorTitle(c.verb!, c.thing!)).toBe(c.title)
    }
  })

  it(`draws invalid input as the 13px line under its field, nothing without a message`, () => {
    const { container, rerender } = render(<FieldError data-testid="err">Enter a full email address</FieldError>)
    const line = screen.getByTestId(`err`)
    expect(line.getAttribute(`role`)).toBe(`alert`)
    expect(line.style.fontSize).toBe(`${FEEDBACK_STATES.errors.inline.size}px`)
    rerender(<FieldError>{null}</FieldError>)
    expect(container.querySelector(`[data-slot=field-error]`)).toBeNull()
  })
})

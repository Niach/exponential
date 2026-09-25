import { useRef } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { issueMenuProps } from "@/components/issue-context-menu/attr"
import { useIssueMenuGestures } from "@/components/issue-context-menu/gestures"

// EXP-1074 — the gesture host's click TAIL. A macOS ctrl+click is a
// right-click, and Firefox follows the `contextmenu` with a `click` on the
// same element; the host swallows that one click itself (document, capture
// phase, ~100 ms), so no row has to check `ctrlKey` — which would make
// ctrl+click a dead click on Windows and Linux.

function Host({
  onOpen,
  onRowClick,
  onOtherClick,
}: {
  onOpen: () => void
  onRowClick: () => void
  onOtherClick: () => void
}) {
  const openRef = useRef(false)
  useIssueMenuGestures(onOpen, openRef)
  return (
    <>
      <div data-testid="row" onClick={onRowClick} {...issueMenuProps(`i1`)}>
        <span data-testid="cell">APP-1</span>
      </div>
      <button type="button" data-testid="other" onClick={onOtherClick}>
        other
      </button>
    </>
  )
}

describe(`useIssueMenuGestures click tail`, () => {
  const onOpen = vi.fn()
  const onRowClick = vi.fn()
  const onOtherClick = vi.fn()

  beforeEach(() => {
    vi.useFakeTimers()
    onOpen.mockReset()
    onRowClick.mockReset()
    onOtherClick.mockReset()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  const draw = () =>
    render(
      <Host onOpen={onOpen} onRowClick={onRowClick} onOtherClick={onOtherClick} />
    )

  it(`swallows the click that follows a contextmenu on the same element`, () => {
    draw()
    fireEvent.contextMenu(screen.getByTestId(`cell`), { clientX: 10, clientY: 10 })
    expect(onOpen).toHaveBeenCalledTimes(1)
    fireEvent.click(screen.getByTestId(`cell`))
    expect(onRowClick).not.toHaveBeenCalled()
    // One shot: the next click is a click.
    fireEvent.click(screen.getByTestId(`row`))
    expect(onRowClick).toHaveBeenCalledTimes(1)
  })

  it(`lets a click through once the tail window has passed`, () => {
    draw()
    fireEvent.contextMenu(screen.getByTestId(`row`), { clientX: 10, clientY: 10 })
    vi.advanceTimersByTime(150)
    fireEvent.click(screen.getByTestId(`row`))
    expect(onRowClick).toHaveBeenCalledTimes(1)
  })

  it(`never touches a click elsewhere, and a plain click stays a click`, () => {
    draw()
    fireEvent.contextMenu(screen.getByTestId(`row`), { clientX: 10, clientY: 10 })
    fireEvent.click(screen.getByTestId(`other`))
    expect(onOtherClick).toHaveBeenCalledTimes(1)
    fireEvent.click(screen.getByTestId(`row`), { ctrlKey: true })
    // The tail is still armed for the row within the window…
    expect(onRowClick).not.toHaveBeenCalled()
    // …and a ctrl+click with no contextmenu before it is an ordinary click.
    fireEvent.click(screen.getByTestId(`row`), { ctrlKey: true })
    expect(onRowClick).toHaveBeenCalledTimes(1)
  })
})

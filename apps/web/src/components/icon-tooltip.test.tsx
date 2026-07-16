import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { IconTooltip } from "@/components/icon-tooltip"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { TooltipProvider } from "@/components/ui/tooltip"

// Radix positions the tooltip with ResizeObserver, which jsdom lacks.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const renderTooltip = (ui: React.ReactNode) => {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  return render(<TooltipProvider delayDuration={0}>{ui}</TooltipProvider>)
}

// Radix opens on focus as well as hover, and focusin bubbles from the inner
// button up to the wrapper span that carries the trigger.
const hover = (element: HTMLElement) => {
  act(() => {
    fireEvent.focus(element)
  })
}

describe(`IconTooltip`, () => {
  it(`reveals the label for an icon-only button`, () => {
    renderTooltip(
      <IconTooltip label="Copy link to issue">
        <Button aria-label="Copy link to issue">icon</Button>
      </IconTooltip>
    )

    expect(screen.queryByRole(`tooltip`)).toBeNull()

    hover(screen.getByLabelText(`Copy link to issue`))

    expect(screen.getByRole(`tooltip`).textContent).toContain(
      `Copy link to issue`
    )
  })

  it(`appends the keyboard shortcut after the label`, () => {
    renderTooltip(
      <IconTooltip label="Next issue" shortcut="J">
        <Button aria-label="Next issue (J)">icon</Button>
      </IconTooltip>
    )

    hover(screen.getByLabelText(`Next issue (J)`))

    expect(screen.getByRole(`tooltip`).textContent).toBe(`Next issueJ`)
  })

  // The reason the trigger wraps a span instead of binding to the button: a
  // disabled button emits no pointer events, which would silently drop the
  // tooltip exactly when the user wants to know why the control is dead.
  it(`still shows the label while the wrapped button is disabled`, async () => {
    const { container } = renderTooltip(
      <IconTooltip label="Previous issue" shortcut="K">
        <Button disabled aria-label="Previous issue (K)">
          icon
        </Button>
      </IconTooltip>
    )

    const trigger = container.querySelector(`span`)

    expect(trigger).toBeTruthy()

    // Hovering the span, NOT the button — the disabled button emits nothing.
    // Radix opens hover tooltips off a timer, so let it flush.
    await act(async () => {
      fireEvent.pointerMove(trigger as HTMLElement, { pointerType: `mouse` })
      await new Promise((resolve) => setTimeout(resolve, 0))
    })

    expect(screen.getByRole(`tooltip`).textContent).toBe(`Previous issueK`)
  })

  // The overflow button is both a tooltip trigger and a menu trigger; the extra
  // span between them must not swallow the click that opens the menu.
  it(`leaves a wrapped dropdown trigger able to open its menu`, () => {
    renderTooltip(
      <IconTooltip label="More actions">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button aria-label="Issue actions">icon</Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuItem>Delete issue</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </IconTooltip>
    )

    expect(screen.queryByText(`Delete issue`)).toBeNull()

    act(() => {
      fireEvent.pointerDown(screen.getByLabelText(`Issue actions`), {
        button: 0,
        ctrlKey: false,
        pointerType: `mouse`,
      })
    })

    expect(screen.getByText(`Delete issue`)).toBeTruthy()
  })
})

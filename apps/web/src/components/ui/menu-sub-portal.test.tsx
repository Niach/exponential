import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

// Radix positions menus with ResizeObserver, which jsdom lacks.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never

// Sub content must render in a portal, OUTSIDE the parent content's DOM.
// Un-portaled it sits inside the content div, whose `glass-panel`
// backdrop-filter makes it the containing block for the fixed-positioned
// submenu — and its overflow-x-hidden then clips the submenu invisible in
// Safari/WebKit, leaving no way to reach "Confirm delete" (EXP-305).
describe(`menu sub content portaling`, () => {
  it(`renders dropdown sub content outside the parent content element`, () => {
    render(
      <DropdownMenu>
        <DropdownMenuTrigger>Open</DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>Delete issue</DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuItem>Confirm delete</DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        </DropdownMenuContent>
      </DropdownMenu>
    )

    act(() => {
      fireEvent.pointerDown(screen.getByText(`Open`), {
        button: 0,
        ctrlKey: false,
        pointerType: `mouse`,
      })
    })

    const subTrigger = screen.getByText(`Delete issue`)
    act(() => {
      subTrigger.focus()
      fireEvent.keyDown(subTrigger, { key: `ArrowRight` })
    })

    const content = document.querySelector(`[data-slot="dropdown-menu-content"]`)
    const sub = document.querySelector(`[data-slot="dropdown-menu-sub-content"]`)

    expect(content).toBeTruthy()
    expect(sub).toBeTruthy()
    expect(content?.contains(sub)).toBe(false)
  })

  it(`renders context-menu sub content outside the parent content element`, () => {
    render(
      <ContextMenu>
        <ContextMenuTrigger>Row</ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuSub>
            <ContextMenuSubTrigger>Delete issue</ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuItem>Confirm delete</ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
        </ContextMenuContent>
      </ContextMenu>
    )

    act(() => {
      fireEvent.contextMenu(screen.getByText(`Row`))
    })

    const subTrigger = screen.getByText(`Delete issue`)
    act(() => {
      subTrigger.focus()
      fireEvent.keyDown(subTrigger, { key: `ArrowRight` })
    })

    const content = document.querySelector(`[data-slot="context-menu-content"]`)
    const sub = document.querySelector(`[data-slot="context-menu-sub-content"]`)

    expect(content).toBeTruthy()
    expect(sub).toBeTruthy()
    expect(content?.contains(sub)).toBe(false)
  })
})

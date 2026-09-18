import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { SidebarMenuButton, SidebarProvider } from "./sidebar"

// EXP-962: the sidebar's compact density is a prop, not a per-file constant.

vi.mock(`./use-mobile`, () => ({ useIsMobile: () => false }))

describe(`SidebarMenuButton density`, () => {
  it(`compact is the 28px row at the list's own type size`, () => {
    render(
      <SidebarProvider>
        <SidebarMenuButton density="compact">Inbox</SidebarMenuButton>
      </SidebarProvider>
    )
    const button = screen.getByRole(`button`, { name: `Inbox` })
    expect(button.getAttribute(`data-density`)).toBe(`compact`)
    expect(button.className).toContain(`h-7`)
    expect(button.className).toContain(`text-sm`)
    expect(button.className).not.toContain(`text-xs`)
  })

  it(`stays 32px by default`, () => {
    render(
      <SidebarProvider>
        <SidebarMenuButton>Inbox</SidebarMenuButton>
      </SidebarProvider>
    )
    const button = screen.getByRole(`button`, { name: `Inbox` })
    expect(button.getAttribute(`data-density`)).toBe(`default`)
    expect(button.className).toContain(`h-8`)
  })
})

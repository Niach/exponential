import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { SIDEBAR_DOT_TONES, SidebarMenuDot } from "@exp/ui"
import designTokens from "../../../../../packages/design-tokens/tokens.json" with { type: "json" }

// #EXP-1480: the rail's status dot is ONE size and ONE palette on web and
// the IDE (`sidebar::rail_dot`, which draws `sidebar.trailingDot` px).
describe(`SidebarMenuDot`, () => {
  it(`is the sidebar.trailingDot token (size-2 = 8 design units)`, () => {
    expect(designTokens.sidebar.trailingDot).toBe(8)
    const { container } = render(<SidebarMenuDot tone="unread" />)
    expect(container.firstElementChild?.className).toContain(`size-2`)
  })

  it(`waits on you in semantic.yellow, like the IDE`, () => {
    // Tailwind yellow-400 IS #FACC15.
    expect(designTokens.semantic.yellow).toBe(`#FACC15`)
    expect(SIDEBAR_DOT_TONES.needsYou).toBe(`bg-yellow-400`)
  })
})

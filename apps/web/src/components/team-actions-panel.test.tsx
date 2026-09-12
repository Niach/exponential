// EXP-858 fallout: the action row's ⋯ menu has two CONDITIONAL rows — Pin
// (sidebar viewports only) and Edit/Delete (owners only) — so a non-owner
// member on a phone viewport would be handed a trigger opening an EMPTY
// popover. The trigger asks first.
import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { SyncedAction } from "@/db/schema"

const mobile = vi.hoisted(() => ({ value: false }))

vi.mock(`@/hooks/use-mobile`, () => ({
  useIsMobile: () => mobile.value,
}))
vi.mock(`@/hooks/use-pins`, () => ({
  usePinToggle: () => ({ pinned: false, toggle: () => {}, busy: false }),
}))
vi.mock(`@/lib/collections`, () => ({
  actionCollection: {},
  automationCollection: {},
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})

const { ActionMenu } = await import(`@/components/team-actions-panel`)

const action = {
  id: `act-1`,
  teamId: `team-1`,
  name: `Ship it`,
} as unknown as SyncedAction

const renderMenu = (isOwner: boolean) =>
  render(
    <ActionMenu
      action={action}
      isOwner={isOwner}
      onEdit={() => {}}
      onDelete={() => {}}
    />
  )

const trigger = () => screen.queryByLabelText(`Action menu for Ship it`)

describe(`the action row's ⋯ menu`, () => {
  it(`renders for an owner on either viewport`, () => {
    mobile.value = false
    const { unmount } = renderMenu(true)
    expect(trigger()).not.toBeNull()
    unmount()
    mobile.value = true
    renderMenu(true)
    expect(trigger()).not.toBeNull()
  })

  it(`renders for a member on a sidebar viewport (Pin is the one row)`, () => {
    mobile.value = false
    renderMenu(false)
    expect(trigger()).not.toBeNull()
  })

  it(`is dropped for a member on a phone viewport — it would be empty`, () => {
    mobile.value = true
    renderMenu(false)
    expect(trigger()).toBeNull()
  })
})

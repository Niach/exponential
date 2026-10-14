import { fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { SidebarProvider } from "@exp/ui"

// EXP-1246: the ONE list-detail host. On the list screen itself nothing is
// selected (no back row); beside a detail the back row returns to the list,
// which stays mounted.

const location = vi.hoisted(() => ({ pathname: `/t/acme/inbox` }))
const navigate = vi.hoisted(() => vi.fn())
const inboxProps = vi.hoisted(() => ({ value: null as null | Record<string, unknown> }))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => navigate,
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location }),
}))
vi.mock(`@/components/team/list-nav`, () => ({
  InboxListNav: (props: Record<string, unknown>) => {
    inboxProps.value = props
    return <div data-testid="inbox-list" />
  },
}))

import {
  ListDetailEmpty,
  ListDetailPane,
  listDetailKey,
} from "@/components/team/list-detail"

function renderPane(origin: Parameters<typeof ListDetailPane>[0][`origin`]) {
  return render(
    <SidebarProvider>
      <ListDetailPane teamSlug="acme" origin={origin} />
    </SidebarProvider>
  )
}

afterEach(() => {
  navigate.mockReset()
  inboxProps.value = null
  location.pathname = `/t/acme/inbox`
})

describe(`ListDetailPane`, () => {
  it(`titles the inbox page without a back row, and drives its tab`, () => {
    renderPane({ kind: `inbox` })
    expect(screen.getByTestId(`inbox-list`)).toBeTruthy()
    expect(screen.queryByRole(`button`, { name: `Back to Inbox` })).toBeNull()
    expect(screen.getByText(`Inbox`)).toBeTruthy()
    const onTabChange = inboxProps.value?.onTabChange as (t: string) => void
    onTabChange(`my-issues`)
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        to: `/t/$teamSlug/inbox`,
        search: { tab: `my-issues` },
        replace: true,
      })
    )
  })

  it(`returns a detail to the inbox, with the tab it came from`, () => {
    location.pathname = `/t/acme/boards/web/issues/MET-1`
    renderPane({ kind: `inbox`, tab: `my-issues` })
    expect(inboxProps.value?.tab).toBe(`my-issues`)
    expect(inboxProps.value?.onTabChange).toBeUndefined()
    fireEvent.click(screen.getByRole(`button`, { name: `Back to Inbox` }))
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        to: `/t/$teamSlug/inbox`,
        search: { tab: `my-issues` },
      })
    )
  })

  it(`keys one host per list`, () => {
    expect(listDetailKey({ kind: `inbox` })).toBe(
      listDetailKey({ kind: `inbox`, tab: `my-issues` })
    )
  })
})

describe(`ListDetailEmpty`, () => {
  it(`names what to pick, per list`, () => {
    const { rerender } = render(<ListDetailEmpty tab="inbox" />)
    expect(screen.getByText(`No notification selected`)).toBeTruthy()
    rerender(<ListDetailEmpty tab="my-issues" />)
    expect(screen.getByText(`No issue selected`)).toBeTruthy()
  })
})

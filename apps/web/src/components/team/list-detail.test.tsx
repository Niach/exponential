import { fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { SidebarProvider } from "@exp/ui"

// EXP-1246: the ONE list-detail host. On the list screen itself nothing is
// selected (no back row for the Inbox; Recent's row shuts the panel); beside a
// detail the back row returns to the list, which stays mounted.

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
vi.mock(`@/components/team/recent-runs-nav`, () => ({
  RecentRunsNav: () => <div data-testid="recent-list" />,
}))

import {
  ListDetailEmpty,
  ListDetailPane,
  listDetailKey,
} from "@/components/team/list-detail"
import {
  setRecentRunsPanelOpen,
  useRecentRunsPanelOpen,
} from "@/lib/recent-runs-panel"
import type { Team } from "@/db/schema"

const team = { id: `t1`, slug: `acme` } as Team

function renderPane(origin: Parameters<typeof ListDetailPane>[0][`origin`]) {
  return render(
    <SidebarProvider>
      <ListDetailPane teamSlug="acme" team={team} origin={origin} currentUserId="u1" />
    </SidebarProvider>
  )
}

afterEach(() => {
  navigate.mockReset()
  inboxProps.value = null
  setRecentRunsPanelOpen(false)
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

  it(`shuts Recent on the Agent page, and returns a run to it`, () => {
    location.pathname = `/t/acme/agent`
    setRecentRunsPanelOpen(true)
    const { unmount } = renderPane({ kind: `agent`, tab: `recent` })
    expect(screen.getByTestId(`recent-list`)).toBeTruthy()
    fireEvent.click(screen.getByRole(`button`, { name: `Back to Agent` }))
    expect(navigate).not.toHaveBeenCalled()
    unmount()

    let open = false
    function Probe() {
      open = useRecentRunsPanelOpen()
      return null
    }
    render(<Probe />)
    expect(open).toBe(false)

    location.pathname = `/t/acme/sessions/s1`
    renderPane({ kind: `agent`, tab: `recent` })
    fireEvent.click(screen.getByRole(`button`, { name: `Back to Agent` }))
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({ to: `/t/$teamSlug/agent` })
    )
  })

  it(`keys one host per list`, () => {
    expect(listDetailKey({ kind: `inbox` })).toBe(
      listDetailKey({ kind: `inbox`, tab: `my-issues` })
    )
    expect(listDetailKey({ kind: `agent`, tab: `recent` })).not.toBe(
      listDetailKey({ kind: `inbox` })
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

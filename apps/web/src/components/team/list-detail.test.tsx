import { render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { SidebarProvider } from "@exp/ui"

// EXP-1246: the ONE list-detail host. #EXP-1672: it is the content card's
// second sidebar, with no title or back row (desktop parity: the main menu
// beside it is the way out). On the list screen it drives the Inbox tab.

const location = vi.hoisted(() => ({
  pathname: `/t/acme/inbox`,
  search: {} as Record<string, unknown>,
}))
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
  SecondSidebarHost,
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
  location.search = {}
})

describe(`ListDetailPane`, () => {
  it(`drives the inbox page's tab, with no title row`, () => {
    renderPane({ kind: `inbox` })
    expect(screen.getByTestId(`inbox-list`)).toBeTruthy()
    expect(screen.queryByRole(`button`, { name: `Back to Inbox` })).toBeNull()
    expect(screen.queryByText(`Inbox`)).toBeNull()
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

  it(`keeps the tab a detail came from, with no back row`, () => {
    location.pathname = `/t/acme/boards/web/issues/MET-1`
    renderPane({ kind: `inbox`, tab: `my-issues` })
    expect(inboxProps.value?.tab).toBe(`my-issues`)
    expect(inboxProps.value?.onTabChange).toBeUndefined()
    expect(screen.queryByRole(`button`, { name: `Back to Inbox` })).toBeNull()
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

// #EXP-1672: the card's [second sidebar | screen] row — the main menu never
// folds for it.
describe(`SecondSidebarHost`, () => {
  function renderHost() {
    return render(
      <SidebarProvider>
        <SecondSidebarHost teamSlug="acme">
          <div data-testid="screen" />
        </SecondSidebarHost>
      </SidebarProvider>
    )
  }

  it(`shows the inbox list beside the inbox page and an inbox detail`, () => {
    renderHost()
    expect(screen.getByTestId(`inbox-list`)).toBeTruthy()
    expect(screen.getByTestId(`screen`)).toBeTruthy()
    expect(screen.getByRole(`separator`, { name: `Resize sidebar` })).toBeTruthy()
  })

  it(`shows the screen alone elsewhere`, () => {
    location.pathname = `/t/acme/boards/web/issues/MET-1`
    location.search = { from: `board:web` }
    renderHost()
    expect(screen.queryByTestId(`inbox-list`)).toBeNull()
    expect(screen.getByTestId(`screen`)).toBeTruthy()
    expect(screen.getByTestId(`second-sidebar`).style.width).toBe(``)
  })
})

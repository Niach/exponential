import { cleanup, fireEvent, render as rtlRender, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import {
  BUILTIN_TOOLS_FILTER_PLACEHOLDER,
  filterBuiltinTools,
  BuiltinToolsGroup,
  TeamMcpServersSection,
} from "@/components/team/mcp-servers-section"
import { contract } from "@exp/domain-contract"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

// EXP-792: the easy MCP page. Each row offers the VIEWER one action from
// their own connection (Connect / Set key / Reconnect / "No sign-in
// needed", or a read-only Connected caption whose actions sit in the row's
// ONE "…" menu, EXP-1285), owner controls are hidden from members, adding from a
// catalog row probes then (for OAuth) connects right away, and the deep
// link's `?connect=` / `?mcp=` one-shots fire once and are consumed.

const mockState = vi.hoisted(() => ({
  list: vi.fn(),
  probe: vi.fn(),
  create: vi.fn(),
  connect: vi.fn(),
  setShared: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
  assign: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    mcpServers: {
      list: { query: mockState.list },
      probe: { mutate: mockState.probe },
      create: { mutate: mockState.create },
      update: { mutate: vi.fn() },
      remove: { mutate: vi.fn() },
      connect: { mutate: mockState.connect },
      setSecret: { mutate: vi.fn() },
      disconnect: { mutate: vi.fn() },
      test: { mutate: vi.fn() },
      setShared: { mutate: mockState.setShared },
    },
  },
}))

vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { success: mockState.toastSuccess, error: mockState.toastError },
}))

const server = (
  id: string,
  auth: string,
  status: string,
  extra: Record<string, unknown> = {}
) => ({
  id,
  teamId: `t1`,
  name: id,
  transport: `http`,
  url: `https://mcp.${id}.app/mcp`,
  headerNames: auth === `secret` ? [`X-Api-Key`] : [],
  command: null,
  args: [],
  envNames: [],
  scopes: [],
  auth,
  enabledByDefault: false,
  createdById: `u1`,
  createdAt: new Date(0).toISOString(),
  updatedAt: new Date(0).toISOString(),
  connection: { status, expiresAt: null, error: null, shared: false },
  connectedCount: status === `connected` ? 1 : 0,
  memberCount: 3,
  sharedCount: 0,
  sharedUserIds: [],
  connectedUserIds: [],
  ...extra,
})

const render = (ui: React.ReactElement) =>
  rtlRender(ui, { wrapper: TooltipProvider })

const rowOf = (name: string) =>
  screen
    .getAllByTestId(`mcp-server-row`)
    .find((row) => row.textContent?.includes(name))!

/** Opens a row's ONE "…" menu (EXP-1285). */
const openMenu = (name: string) =>
  fireEvent.pointerDown(screen.getByLabelText(`Server menu for ${name}`), {
    button: 0,
    pointerType: `mouse`,
  })

beforeEach(() => {
  for (const fn of Object.values(mockState)) fn.mockReset()
  vi.stubGlobal(`location`, { ...window.location, assign: mockState.assign, pathname: `/t/acme/settings/mcp-servers` })
})

describe(`TeamMcpServersSection`, () => {
  // EXP-1274: the unslopped pane — no subtitle, no Refresh, the shared
  // list empty state, Add server for an owner even on an empty team, and the
  // built-in tools always expanded behind an inline filter.
  it(`draws the empty team as ONE list empty state with Add server and the catalog`, async () => {
    mockState.list.mockResolvedValue([])
    const { container } = render(<TeamMcpServersSection teamId="t1" isOwner />)
    await screen.findByText(`No MCP servers yet`)
    const empty = container.querySelector(`[data-slot="empty-state"][data-size="list"]`)!
    expect(empty.textContent).toBe(`No MCP servers yet`)
    expect(screen.getByText(`Add server`)).toBeTruthy()
    expect(screen.getByPlaceholderText(`Search or paste a server URL`)).toBeTruthy()
    expect(container.textContent).not.toContain(`Tools your agents can use`)
    expect(container.textContent).not.toContain(`Add one to offer it`)
    expect(screen.queryByRole(`button`, { name: `Refresh` })).toBeNull()
  })

  it(`lists every built-in tool expanded, with an inline filter and no count`, async () => {
    // #VAPP-185: the group is Settings › MCP's last section, its own component.
    const { container } = render(<BuiltinToolsGroup />)
    const total = contract.expToolDisplay.tools.length
    const band = screen.getByText(`Built-in Exponential tools`).closest(`[data-slot="glass-section-header"]`)!
    expect(band.tagName).toBe(`DIV`)
    expect(band.getAttribute(`aria-expanded`)).toBeNull()
    expect(band.querySelector(`[data-part="count"]`)).toBeNull()
    expect(screen.getAllByTitle(/^exponential_/).length).toBe(total)

    const filter = screen.getByPlaceholderText(BUILTIN_TOOLS_FILTER_PLACEHOLDER)
    fireEvent.change(filter, { target: { value: `issues_create` } })
    expect(screen.getAllByTitle(/^exponential_/).length).toBe(1)
    expect(screen.getByTitle(`exponential_issues_create`)).toBeTruthy()

    fireEvent.change(filter, { target: { value: `zzz-nothing` } })
    expect(screen.queryAllByTitle(/^exponential_/).length).toBe(0)
    expect(container.querySelectorAll(`[data-slot="empty-state"][data-size="list"]`).length).toBe(1)
    expect(screen.getByText(`No tools match`)).toBeTruthy()
  })

  it(`gives each row the viewer's one action and hides owner controls from members`, async () => {
    mockState.list.mockResolvedValue([
      server(`linear`, `oauth`, `connected`),
      server(`sentry`, `oauth`, `not_connected`),
      server(`stripe`, `secret`, `not_connected`),
      server(`notion`, `oauth`, `error`, {
        connection: { status: `error`, expiresAt: null, error: `refresh failed` },
      }),
      server(`docs`, `none`, `not_needed`),
    ])
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`linear`)

    expect(rowOf(`linear`).textContent).toContain(`Connected`)
    // ×2 anatomy: name · Default pill · host — no connection counts.
    expect(rowOf(`linear`).textContent).not.toContain(`of 3 connected`)
    expect(rowOf(`sentry`).textContent).toContain(`Connect`)
    expect(rowOf(`stripe`).textContent).toContain(`Set key`)
    expect(rowOf(`notion`).textContent).toContain(`Reconnect`)
    expect(rowOf(`docs`).textContent).toContain(`No sign-in needed`)
    expect(rowOf(`docs`).textContent).not.toContain(`connected`)
    // EXP-1285: Connected is a caption, not a button; a member's only menu
    // is the connected row's (its connection items), never the others'.
    expect(screen.queryByRole(`button`, { name: /Connected/ })).toBeNull()
    expect(screen.getAllByLabelText(/Server menu for/).map((el) => el.getAttribute(`aria-label`))).toEqual([
      `Server menu for linear`,
    ])
    expect(screen.queryByText(`Add server`)).toBeNull()
  })

  // EXP-1285: ONE "…" menu per row — the viewer's connection items with the
  // owner's Edit above the separator, Disconnect / Remove below it.
  it(`merges the connection items and the owner's Edit / Remove into the row menu`, async () => {
    mockState.list.mockResolvedValue([
      server(`linear`, `oauth`, `connected`),
      server(`sentry`, `oauth`, `not_connected`),
    ])
    render(<TeamMcpServersSection teamId="t1" isOwner />)
    await screen.findByText(`linear`)

    openMenu(`linear`)
    await screen.findByRole(`menuitem`, { name: `Test connection` })
    expect(screen.getAllByRole(`menuitem`).map((item) => item.textContent)).toEqual([
      `Test connection`,
      `Share with team`,
      `Edit`,
      `Disconnect`,
      `Remove`,
    ])
    fireEvent.keyDown(document.activeElement ?? document.body, { key: `Escape` })

    openMenu(`sentry`)
    await screen.findByRole(`menuitem`, { name: `Edit` })
    expect(screen.getAllByRole(`menuitem`).map((item) => item.textContent)).toEqual([
      `Edit`,
      `Remove`,
    ])
  })

  it(`sends Connect to the provider with this page as the way back`, async () => {
    mockState.list.mockResolvedValue([server(`sentry`, `oauth`, `not_connected`)])
    mockState.connect.mockResolvedValue({ authorizeUrl: `https://auth.example/authorize` })
    render(<TeamMcpServersSection teamId="t1" isOwner />)
    await screen.findByText(`sentry`)
    fireEvent.click(screen.getByRole(`button`, { name: /Connect/ }))
    await waitFor(() =>
      expect(mockState.assign).toHaveBeenCalledWith(`https://auth.example/authorize`)
    )
    expect(mockState.connect).toHaveBeenCalledWith(
      { serverId: `sentry`, returnTo: `/t/acme/settings/mcp-servers` },
      expect.anything()
    )
  })

  it(`refuses to navigate to a non-http(s) authorize URL`, async () => {
    mockState.list.mockResolvedValue([server(`sentry`, `oauth`, `not_connected`)])
    mockState.connect.mockResolvedValue({ authorizeUrl: `javascript:alert(1)` })
    render(<TeamMcpServersSection teamId="t1" isOwner />)
    await screen.findByText(`sentry`)
    fireEvent.click(screen.getByRole(`button`, { name: /Connect/ }))
    await waitFor(() =>
      expect(mockState.toastError).toHaveBeenCalledWith(
        `Couldn't connect to sentry`,
        expect.objectContaining({ description: expect.stringMatching(/http\(s\)/) })
      )
    )
    expect(mockState.assign).not.toHaveBeenCalled()
  })

  it(`probes a pasted URL from the same search box`, async () => {
    mockState.list.mockResolvedValue([])
    mockState.probe.mockResolvedValue({
      url: `https://mcp.example.com/mcp`,
      suggestedName: `example`,
      auth: `none`,
      reachable: true,
      error: null,
      scopes: [],
    })
    render(<TeamMcpServersSection teamId="t1" isOwner />)

    fireEvent.change(
      await screen.findByPlaceholderText(`Search or paste a server URL`),
      { target: { value: `https://mcp.example.com/mcp` } }
    )
    fireEvent.click(
      await screen.findByRole(`option`, { name: /Use mcp\.example\.com/ })
    )
    await waitFor(() =>
      expect(mockState.probe).toHaveBeenCalledWith(
        { teamId: `t1`, url: `https://mcp.example.com/mcp` },
        expect.anything()
      )
    )
    expect((await screen.findByLabelText(`Name`) as HTMLInputElement).value).toBe(`example`)
  })

  it(`adds a catalog server on an empty team and connects the owner at once`, async () => {
    mockState.list.mockResolvedValue([])
    mockState.probe.mockResolvedValue({
      url: `https://mcp.linear.app/mcp`,
      suggestedName: `linear`,
      auth: `oauth`,
      reachable: true,
      error: null,
      scopes: [],
    })
    mockState.create.mockResolvedValue(server(`linear`, `oauth`, `not_connected`))
    mockState.connect.mockResolvedValue({ authorizeUrl: `https://linear.app/oauth` })
    render(<TeamMcpServersSection teamId="t1" isOwner />)

    fireEvent.click(await screen.findByRole(`option`, { name: /Linear/ }))
    await screen.findByText(`Members sign in with their own account (OAuth)`)
    expect((screen.getByLabelText(`Name`) as HTMLInputElement).value).toBe(`linear`)
    fireEvent.click(screen.getByRole(`button`, { name: `Save and connect` }))

    await waitFor(() =>
      expect(mockState.assign).toHaveBeenCalledWith(`https://linear.app/oauth`)
    )
    expect(mockState.create).toHaveBeenCalledWith(
      expect.objectContaining({
        teamId: `t1`,
        name: `linear`,
        url: `https://mcp.linear.app/mcp`,
        auth: `oauth`,
        enabledByDefault: true,
      }),
      expect.anything()
    )
  })

  it(`opens a template entry on the form, unprobed, with the URL field focused`, async () => {
    mockState.list.mockResolvedValue([])
    render(<TeamMcpServersSection teamId="t1" isOwner />)

    const row = await screen.findByRole(`option`, { name: /GitLab \(self-managed\)/ })
    expect(row.textContent).toContain(`Your own host`)
    // A template is never "Added" and never disabled.
    expect(row.getAttribute(`aria-disabled`)).not.toBe(`true`)
    fireEvent.click(row)

    const url = (await screen.findByLabelText(`URL`)) as HTMLInputElement
    expect(url.value).toBe(`https://{host}/api/v4/mcp`)
    expect(document.activeElement).toBe(url)
    expect((screen.getByLabelText(`Name`) as HTMLInputElement).value).toBe(`gitlab-self-managed`)
    expect(mockState.probe).not.toHaveBeenCalled()
    await screen.findByText(`Members sign in with their own account (OAuth)`)
  })

  it(`marks an added catalog server and draws its brand mark on the row`, async () => {
    mockState.list.mockResolvedValue([
      server(`linear`, `oauth`, `connected`, { url: `https://mcp.linear.app/mcp` }),
    ])
    render(<TeamMcpServersSection teamId="t1" isOwner />)
    await screen.findByText(`linear`)
    // The settings row wears the Linear mark (512 grid, its own fill), not a concept glyph.
    const mark = rowOf(`linear`).querySelector(`svg`)!
    expect(mark.getAttribute(`viewBox`)).toBe(`0 0 512 512`)
    expect(mark.getAttribute(`class`)).toContain(`size-4`)
    expect(mark.querySelector(`path`)?.getAttribute(`fill`)).toBe(`#fff`)

    fireEvent.click(screen.getByRole(`button`, { name: `Add server` }))
    const added = await screen.findByRole(`option`, { name: /Linear/ })
    expect(added.textContent).toContain(`Added`)
    expect(added.getAttribute(`aria-disabled`)).toBe(`true`)
    // Every catalog row leads with a brand mark.
    const rows = screen.getAllByTestId(`mcp-catalog-row`)
    expect(rows.length).toBeGreaterThanOrEqual(19)
    for (const row of rows) {
      expect(row.querySelector(`svg`)?.getAttribute(`viewBox`)).toBe(`0 0 512 512`)
    }
  })

  it(`toasts the callback verdict once and consumes the request`, async () => {
    mockState.list.mockResolvedValue([server(`linear`, `oauth`, `connected`)])
    const consumed = vi.fn()
    render(
      <TeamMcpServersSection
        teamId="t1"
        isOwner
        request={{ mcp: `connected`, server: `linear` }}
        onRequestConsumed={consumed}
      />
    )
    await waitFor(() =>
      expect(mockState.toastSuccess).toHaveBeenCalledWith(`Connected to linear`)
    )
    expect(consumed).toHaveBeenCalledTimes(1)
  })

  it(`starts the connect a ?connect= deep link names`, async () => {
    mockState.list.mockResolvedValue([server(`sentry`, `oauth`, `not_connected`)])
    mockState.connect.mockResolvedValue({ authorizeUrl: `https://sentry.io/oauth` })
    render(
      <TeamMcpServersSection
        teamId="t1"
        isOwner={false}
        request={{ connect: `sentry` }}
        onRequestConsumed={vi.fn()}
      />
    )
    await waitFor(() =>
      expect(mockState.assign).toHaveBeenCalledWith(`https://sentry.io/oauth`)
    )
  })

  // FEED-73: sharing stays in the row menu; the row carries no "Shared"
  // pill or "K shared" count (name · Default pill · host only).
  it(`keeps share state off the row`, async () => {
    mockState.list.mockResolvedValue([
      server(`linear`, `oauth`, `connected`, {
        connection: { status: `connected`, expiresAt: null, error: null, shared: true },
        connectedCount: 2,
        sharedCount: 1,
      }),
    ])
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`linear`)

    expect(rowOf(`linear`).textContent).not.toContain(`shared`)
    expect(rowOf(`linear`).textContent).not.toContain(`Shared`)
  })

  it(`flips the row menu's share item and sends it`, async () => {
    mockState.list.mockResolvedValue([server(`sentry`, `oauth`, `connected`)])
    mockState.setShared.mockResolvedValue({ status: `connected`, shared: true })
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`sentry`)

    openMenu(`sentry`)
    const share = await screen.findByRole(`menuitem`, { name: `Share with team` })
    expect(screen.queryByRole(`menuitem`, { name: `Stop sharing` })).toBeNull()

    mockState.list.mockResolvedValue([
      server(`sentry`, `oauth`, `connected`, {
        connection: { status: `connected`, expiresAt: null, error: null, shared: true },
        sharedCount: 1,
      }),
    ])
    fireEvent.click(share)
    await waitFor(() =>
      expect(mockState.setShared).toHaveBeenCalledWith(
        { serverId: `sentry`, shared: true },
        expect.anything()
      )
    )
    await waitFor(() =>
      expect(mockState.toastSuccess).toHaveBeenCalled()
    )
    expect(mockState.toastSuccess).toHaveBeenCalledWith(
      `Shared sentry with the team: action sessions on teammates' machines use your connection`
    )

    openMenu(`sentry`)
    expect(
      await screen.findByRole(`menuitem`, { name: `Stop sharing` })
    ).toBeTruthy()
    expect(screen.queryByRole(`menuitem`, { name: `Share with team` })).toBeNull()
  })

  // A stdio server's secret stays on its owner's machine: no Share item,
  // but an already-shared row (before the server refused it) can be undone.
  it(`offers no Share item on a stdio row, only Stop sharing on a stale shared one`, async () => {
    const stdio = (extra: Record<string, unknown> = {}) =>
      server(`local`, `secret`, `connected`, {
        transport: `stdio`,
        url: null,
        command: `npx`,
        args: [`-y`, `@acme/mcp`],
        headerNames: [],
        envNames: [`ACME_TOKEN`],
        ...extra,
      })
    mockState.list.mockResolvedValue([stdio()])
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`local`)

    openMenu(`local`)
    expect(await screen.findByRole(`menuitem`, { name: `Replace key` })).toBeTruthy()
    expect(screen.queryByRole(`menuitem`, { name: `Share with team` })).toBeNull()
    expect(screen.queryByRole(`menuitem`, { name: `Stop sharing` })).toBeNull()
    fireEvent.keyDown(document.activeElement ?? document.body, { key: `Escape` })

    mockState.list.mockResolvedValue([
      stdio({
        connection: { status: `connected`, expiresAt: null, error: null, shared: true },
      }),
    ])
    mockState.setShared.mockResolvedValue({ status: `connected`, shared: false })
    cleanup()
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`local`)
    openMenu(`local`)
    const stop = await screen.findByRole(`menuitem`, { name: `Stop sharing` })
    expect(screen.queryByRole(`menuitem`, { name: `Share with team` })).toBeNull()
    fireEvent.click(stop)
    await waitFor(() =>
      expect(mockState.setShared).toHaveBeenCalledWith(
        { serverId: `local`, shared: false },
        expect.anything()
      )
    )
    expect(mockState.toastSuccess).toHaveBeenCalledWith(`Stopped sharing local`)
  })
})

describe(`filterBuiltinTools`, () => {
  const tools = [
    { name: `exponential_issues_create`, title: `Create issue`, blurb: `Adds an issue to a board.` },
    { name: `exponential_boards_list`, title: `List boards`, blurb: `Lists the boards you can see.` },
  ]
  it(`matches the title, the blurb and the wire name, case-insensitively`, () => {
    expect(filterBuiltinTools(tools, ``).length).toBe(2)
    expect(filterBuiltinTools(tools, `  `).length).toBe(2)
    expect(filterBuiltinTools(tools, `CREATE`).map((tool) => tool.name)).toEqual([`exponential_issues_create`])
    expect(filterBuiltinTools(tools, `you can see`).map((tool) => tool.name)).toEqual([`exponential_boards_list`])
    expect(filterBuiltinTools(tools, `boards_list`).map((tool) => tool.name)).toEqual([`exponential_boards_list`])
    expect(filterBuiltinTools(tools, `nope`)).toEqual([])
  })
})

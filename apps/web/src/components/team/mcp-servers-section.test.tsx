import { cleanup, fireEvent, render as rtlRender, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import {
  BUILTIN_TOOLS_FILTER_PLACEHOLDER,
  filterBuiltinTools,
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
  setSecret: vi.fn(),
  tools: vi.fn(),
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
      setSecret: { mutate: mockState.setSecret },
      disconnect: { mutate: vi.fn() },
      test: { mutate: vi.fn() },
      setShared: { mutate: mockState.setShared },
      tools: { query: mockState.tools },
    },
  },
}))

// The real hook subscribes to better-auth's session atom, whose refresh
// cleanup runs on a timer AFTER jsdom has torn the window down (an uncaught
// `window is not defined` that failed the shard). No test here reads the
// session (the "Runs on" picker only lists the viewer's own devices).
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: null }) }))

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
  // Owner 2026-11-12 (fixtures/settings-mcp.json): ONE Servers group, the
  // built-ins first (copy, no menu), then the team's servers; an owner with
  // none gets the catalog under the group.
  it(`lists the built-ins first and gives an owner with no server the catalog`, async () => {
    mockState.list.mockResolvedValue([])
    const { container } = render(<TeamMcpServersSection teamId="t1" isOwner />)
    await screen.findByPlaceholderText(`Search or paste a server URL`)
    const builtin = screen.getByTestId(`settings-mcp-builtin`)
    expect(builtin.textContent).toContain(`Exponential`)
    expect(builtin.textContent).toContain(`Built-in`)
    expect(builtin.textContent).toContain(`/api/mcp`)
    expect(builtin.textContent).toContain(`Copy`)
    expect(builtin.querySelector(`[aria-label^="Server menu"]`)).toBeNull()
    expect(screen.queryByTestId(`settings-mcp-apps-server`)).toBeNull()
    expect(screen.getByText(`Add server`)).toBeTruthy()
    expect(container.querySelector(`[data-slot="empty-state"]`)).toBeNull()
    expect(screen.queryByRole(`button`, { name: `Refresh` })).toBeNull()
  })

  it(`shows Team apps when an app exposes tools, its tools one band per app`, async () => {
    mockState.list.mockResolvedValue([server(`linear`, `oauth`, `connected`)])
    const appTools = [
      { id: `a1`, slug: `crm`, name: `Customer admin`, tools: [{ name: `crm_find`, title: `Find customer`, blurb: `Finds one.` }] },
    ]
    render(<TeamMcpServersSection teamId="t1" isOwner={false} appTools={appTools} />)
    await screen.findByText(`linear`)
    const order = screen.getAllByTestId(`mcp-server-disclosure`).map((el) => el.textContent)
    expect(order[0]).toContain(`Exponential`)
    expect(order[1]).toContain(`Team apps`)
    expect(order[2]).toContain(`linear`)
    expect(screen.getByTestId(`settings-mcp-apps-server`).textContent).toContain(`/api/mcp/apps`)
    expect(screen.queryByTitle(`crm_find`)).toBeNull()
    fireEvent.click(screen.getByLabelText(`Show Team apps tools`))
    expect(screen.getByTitle(`crm_find`).textContent).toContain(`Find customer`)
    expect(screen.getByText(`Customer admin`)).toBeTruthy()
  })

  it(`unfolds Exponential to every built-in tool, one band per group, behind one filter`, async () => {
    // EXP-1657: one band per contract tool group; aliases never listed.
    mockState.list.mockResolvedValue([])
    const { container } = render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    expect(screen.queryAllByTitle(/^exponential_/).length).toBe(0)
    fireEvent.click(screen.getByLabelText(`Show Exponential tools`))
    const total = contract.expToolDisplay.tools.filter((row) => !(`aliasOf` in row)).length
    const labels = contract.expToolDisplay.groups.map((group) => group.label)
    const bands = [...container.querySelectorAll(`[data-band]`)]
    expect(bands.map((band) => band.getAttribute(`data-band`))).toEqual(contract.expToolDisplay.groups.map((g) => g.id))
    expect(bands.map((band) => band.firstElementChild?.textContent)).toEqual(labels)
    expect(screen.getAllByPlaceholderText(BUILTIN_TOOLS_FILTER_PLACEHOLDER).length).toBe(1)
    expect(screen.getAllByTitle(/^exponential_/).length).toBe(total)

    const filter = screen.getByPlaceholderText(BUILTIN_TOOLS_FILTER_PLACEHOLDER)
    fireEvent.change(filter, { target: { value: `issues_create` } })
    expect(screen.getAllByTitle(/^exponential_/).length).toBe(1)
    expect(screen.getByTitle(`exponential_issues_create`)).toBeTruthy()
    expect(container.querySelectorAll(`[data-band]`).length).toBe(1)

    fireEvent.change(filter, { target: { value: `notif` } })
    const left = [...container.querySelectorAll(`[data-band]`)]
    expect(left.map((band) => band.getAttribute(`data-band`))).toEqual([`activity`])

    fireEvent.change(filter, { target: { value: `zzz-nothing` } })
    expect(screen.queryAllByTitle(/^exponential_/).length).toBe(0)
    expect(container.querySelectorAll(`[data-slot="empty-state"][data-size="list"]`).length).toBe(1)
    expect(screen.getByText(`No tools match`)).toBeTruthy()

    fireEvent.click(screen.getByLabelText(`Hide Exponential tools`))
    expect(screen.queryByPlaceholderText(BUILTIN_TOOLS_FILTER_PLACEHOLDER)).toBeNull()
  })

  // `toolsStateCases`: a connected http server lists on first open (flat),
  // a failure says why with Retry, unconnected / stdio rows say so.
  it(`lists a team server's tools on open, with retry on failure`, async () => {
    mockState.list.mockResolvedValue([
      server(`linear`, `oauth`, `connected`),
      server(`sentry`, `oauth`, `not_connected`),
      server(`local`, `none`, `not_needed`, { transport: `stdio`, url: null, command: `npx local` }),
    ])
    mockState.tools
      .mockResolvedValueOnce({ ok: false, tools: null, error: `HTTP 500` })
      .mockResolvedValueOnce({ ok: true, tools: [{ name: `list_issues`, title: `List issues`, blurb: `Lists them.` }], error: null })
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`linear`)
    expect(mockState.tools).not.toHaveBeenCalled()
    fireEvent.click(screen.getByLabelText(`Show linear tools`))
    await screen.findByText(`Could not list its tools: HTTP 500`)
    expect(mockState.tools).toHaveBeenCalledWith({ serverId: `linear` }, expect.anything())
    fireEvent.click(screen.getByRole(`button`, { name: `Retry` }))
    expect((await screen.findByTitle(`list_issues`)).textContent).toContain(`List issues`)

    fireEvent.click(screen.getByLabelText(`Show sentry tools`))
    expect(screen.getByText(`Connect to see its tools.`)).toBeTruthy()
    fireEvent.click(screen.getByLabelText(`Show local tools`))
    expect(screen.getByText(`A local command: its tools list where a session runs it.`)).toBeTruthy()
    expect(mockState.tools).toHaveBeenCalledTimes(2)
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
  // #EXP-1793: a secret server may declare several headers (Cloudflare
  // Access's two): Set key asks for one masked value per name, labelled by
  // it, and sends them keyed by name.
  it(`asks a multi-header secret server for one value per header`, async () => {
    mockState.list.mockResolvedValue([
      server(`gate`, `secret`, `not_connected`, {
        headerNames: [`CF-Access-Client-Id`, `CF-Access-Client-Secret`],
      }),
    ])
    mockState.setSecret.mockResolvedValue({ ok: true })
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`gate`)
    fireEvent.click(screen.getByRole(`button`, { name: /Set key/ }))

    const id = await screen.findByLabelText(`CF-Access-Client-Id`)
    const secret = screen.getByLabelText(`CF-Access-Client-Secret`)
    const save = screen.getByRole(`button`, { name: `Save key` })
    fireEvent.change(id, { target: { value: ` abc.access ` } })
    // One value is not enough: every declared name needs its own.
    expect((save as HTMLButtonElement).disabled).toBe(true)
    fireEvent.change(secret, { target: { value: `s3cret` } })
    expect((save as HTMLButtonElement).disabled).toBe(false)
    fireEvent.click(save)
    await waitFor(() =>
      expect(mockState.setSecret).toHaveBeenCalledWith(
        {
          serverId: `gate`,
          values: { "CF-Access-Client-Id": `abc.access`, "CF-Access-Client-Secret": `s3cret` },
        },
        expect.anything()
      )
    )
  })

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
    const mark = rowOf(`linear`).querySelector(`svg:not([data-slot="disclosure-chevron"])`)!
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

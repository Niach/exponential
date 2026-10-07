import { cleanup, fireEvent, render as rtlRender, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import { TeamMcpServersSection } from "@/components/team/mcp-servers-section"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

// EXP-792: the easy MCP page. Each row offers the VIEWER one action from
// their own connection (Connect / Set key / Connected / Reconnect / "No
// sign-in needed"), owner controls are hidden from members, adding from a
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

beforeEach(() => {
  for (const fn of Object.values(mockState)) fn.mockReset()
  vi.stubGlobal(`location`, { ...window.location, assign: mockState.assign, pathname: `/t/acme/settings/mcp-servers` })
})

describe(`TeamMcpServersSection`, () => {
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
    expect(rowOf(`linear`).textContent).toContain(`1 of 3 connected`)
    expect(rowOf(`sentry`).textContent).toContain(`Connect`)
    expect(rowOf(`stripe`).textContent).toContain(`Set key`)
    expect(rowOf(`notion`).textContent).toContain(`Reconnect`)
    expect(rowOf(`docs`).textContent).toContain(`No sign-in needed`)
    expect(rowOf(`docs`).textContent).not.toContain(`connected`)
    expect(screen.queryByLabelText(/Server menu for/)).toBeNull()
    expect(screen.queryByText(`Add server`)).toBeNull()
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
        `Could not connect to sentry`,
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

  // FEED-73: sharing a connection with the team, like a shared device.
  it(`counts shared connections and pills the viewer's own shared one`, async () => {
    mockState.list.mockResolvedValue([
      server(`linear`, `oauth`, `connected`, {
        connection: { status: `connected`, expiresAt: null, error: null, shared: true },
        connectedCount: 2,
        sharedCount: 1,
      }),
      server(`sentry`, `oauth`, `connected`),
    ])
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`linear`)

    expect(rowOf(`linear`).textContent).toContain(`2 of 3 connected · 1 shared`)
    expect(rowOf(`linear`).textContent).toContain(`Shared`)
    expect(rowOf(`sentry`).textContent).toContain(`1 of 3 connected`)
    expect(rowOf(`sentry`).textContent).not.toContain(`shared`)
    expect(rowOf(`sentry`).textContent).not.toContain(`Shared`)
  })

  it(`flips the Connected menu's share item and sends it`, async () => {
    mockState.list.mockResolvedValue([server(`sentry`, `oauth`, `connected`)])
    mockState.setShared.mockResolvedValue({ status: `connected`, shared: true })
    render(<TeamMcpServersSection teamId="t1" isOwner={false} />)
    await screen.findByText(`sentry`)

    fireEvent.pointerDown(screen.getByLabelText(`Connected to sentry`), {
      button: 0,
      pointerType: `mouse`,
    })
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
      expect(rowOf(`sentry`).textContent).toContain(`1 shared`)
    )
    expect(mockState.toastSuccess).toHaveBeenCalledWith(
      `Shared sentry with the team: action runs on teammates' machines use your connection`
    )

    fireEvent.pointerDown(screen.getByLabelText(`Connected to sentry`), {
      button: 0,
      pointerType: `mouse`,
    })
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

    fireEvent.pointerDown(screen.getByLabelText(`Connected to local`), {
      button: 0,
      pointerType: `mouse`,
    })
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
    fireEvent.pointerDown(screen.getByLabelText(`Connected to local`), {
      button: 0,
      pointerType: `mouse`,
    })
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

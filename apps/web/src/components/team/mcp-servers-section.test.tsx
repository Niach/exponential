import { fireEvent, render as rtlRender, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import { TeamMcpServersSection } from "@/components/team/mcp-servers-section"

// EXP-792: the easy MCP page. Each row offers the VIEWER one action from
// their own connection (Connect / Set key / Connected / Reconnect / "No
// sign-in needed"), owner controls are hidden from members, adding from a
// catalog tile probes then (for OAuth) connects right away, and the deep
// link's `?connect=` / `?mcp=` one-shots fire once and are consumed.

const mockState = vi.hoisted(() => ({
  list: vi.fn(),
  probe: vi.fn(),
  create: vi.fn(),
  connect: vi.fn(),
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
    },
  },
}))

vi.mock(`sonner`, () => ({
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
  connection: { status, expiresAt: null, error: null },
  connectedCount: status === `connected` ? 1 : 0,
  memberCount: 3,
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

    fireEvent.click(await screen.findByRole(`button`, { name: /Linear/ }))
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
})

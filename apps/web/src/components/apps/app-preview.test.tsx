// #VAPP-251 (fixtures/apps.json A PACKAGE THAT NO LONGER VALIDATES): a package
// the host refuses lands in the pane's own state, never at the root; a
// republished one repaints without a reload; the shell keeps its own errors.

import { Component, type ReactNode } from "react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, render, screen, waitFor } from "@testing-library/react"
import { APP_SHARE_COPY } from "@exp/domain-contract/app-share"
import { APPS_COPY, APPS_TEST_IDS } from "@exp/domain-contract/apps"

const { params, router } = vi.hoisted(() => ({ params: { token: `t1` }, router: { history: { push: () => {} } } }))

vi.mock(`@/lib/collections`, () => {
  const collection = () => ({
    toArray: [],
    subscribeChanges: () => ({ unsubscribe: () => {} }),
    preload: () => Promise.resolve(),
    utils: { awaitTxId: () => Promise.resolve() },
  })
  return {
    deviceCollection: collection(),
    issueCollection: collection(),
    boardCollection: collection(),
    teamCollection: collection(),
    appCollection: collection(),
    appRecordCollection: collection(),
    userCollection: collection(),
    teamMemberCollection: collection(),
    sessionCollection: collection(),
    commentCollection: collection(),
    attachmentCollection: collection(),
    issueEventCollection: collection(),
    sessionIssueCollection: collection(),
  }
})
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@tanstack/react-router`, async (importOriginal) => ({
  ...((await importOriginal()) as Record<string, unknown>),
  useRouter: () => router,
  createFileRoute: () => (options: Record<string, unknown>) => ({ options, useParams: () => params }),
}))

import { SidebarProvider } from "@exp/ui"
import { CORE_CATALOG_ID, PACKAGE_FORMAT_VERSION, upgradePackageBody09 } from "@exponential_app/eui"
import fixture from "@/lib/apps/a2ui10-migrate.fixture.json"
import { AppPreview } from "./app-preview"
import { AppSurfaceBoundary } from "./app-surface-boundary"
import { Route as GuestRoute } from "@/routes/s/$token"

const usable = (title: string): Record<string, unknown> => ({
  formatVersion: PACKAGE_FORMAT_VERSION,
  id: `acme.hours`,
  name: `Hours`,
  version: `1.0.0`,
  catalogId: CORE_CATALOG_ID,
  templates: { main: { components: [{ id: `root`, component: `Text`, text: title }] } },
})
/** The issue's shape: valid when published, refused since a rule tightened. */
const refused = { ...usable(`never`), records: { lead: { schema: { type: `object`, properties: { email: { type: `string`, enum: [] } } } } } }
const REFUSAL = /is unusable/

class Root extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null }
  static getDerivedStateFromError(error: Error) {
    return { error: error.message }
  }
  render() {
    return this.state.error ? <p data-testid="root-error">{this.state.error}</p> : this.props.children
  }
}

beforeEach(() => {
  vi.spyOn(console, `error`).mockImplementation(() => {})
})
afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe(`AppPreview on a refused package`, () => {
  it(`paints copy.needsUpdate in the pane; the shell stays and the message goes to the log`, () => {
    render(
      <Root>
        <nav data-testid="shell" />
        <AppPreview variant="run" pkg={refused} />
      </Root>
    )
    expect(screen.getByTestId(APPS_TEST_IDS.run).textContent).toContain(APPS_COPY.needsUpdate)
    expect(screen.getByTestId(`shell`)).toBeTruthy()
    expect(screen.queryByTestId(`root-error`)).toBeNull()
    expect(document.body.textContent).not.toMatch(REFUSAL)
    expect(vi.mocked(console.error).mock.calls.some(([first, second]) => first === `[apps] the app surface failed` && REFUSAL.test(String((second as Error).message)))).toBe(true)
  })

  it(`the editor's frame alike, and a republished package repaints without a remount`, async () => {
    const view = render(<AppPreview pkg={refused} />)
    expect(screen.getByTestId(APPS_TEST_IDS.preview).textContent).toContain(APPS_COPY.needsUpdate)
    view.rerender(<AppPreview pkg={usable(`Back again`)} />)
    await waitFor(() => expect(screen.getByTestId(APPS_TEST_IDS.preview).textContent).toContain(`Back again`))
    expect(document.body.textContent).not.toContain(APPS_COPY.needsUpdate)
  })

  it(`no package stays the empty state, a usable one its surface`, async () => {
    const view = render(<AppPreview pkg={null} />)
    expect(document.body.textContent).toContain(APPS_COPY.previewEmpty)
    view.rerender(<AppPreview pkg={usable(`Painted`)} />)
    await waitFor(() => expect(document.body.textContent).toContain(`Painted`))
  })
})

describe(`AppPreview re-opening`, () => {
  const two = { ...usable(`One`), templates: { one: { components: [{ id: `root`, component: `Text`, text: `First` }] }, two: { components: [{ id: `root`, component: `Text`, text: `Second` }] } } }

  it(`round 9 §1.4: navigating twice paints the second screen on the same host`, async () => {
    const reports = vi.spyOn(console, `warn`).mockImplementation(() => {})
    const view = render(<AppPreview pkg={two} template="one" />)
    await waitFor(() => expect(screen.getByTestId(APPS_TEST_IDS.preview).textContent).toContain(`First`))
    view.rerender(<AppPreview pkg={two} template="two" />)
    await waitFor(() => expect(screen.getByTestId(APPS_TEST_IDS.preview).textContent).toContain(`Second`))
    expect(screen.getByTestId(APPS_TEST_IDS.preview).textContent).not.toContain(`First`)
    view.rerender(<AppPreview pkg={two} template="one" />)
    await waitFor(() => expect(screen.getByTestId(APPS_TEST_IDS.preview).textContent).toContain(`First`))
    view.rerender(<AppPreview pkg={two} template="two" />)
    await waitFor(() => expect(screen.getByTestId(APPS_TEST_IDS.preview).textContent).toContain(`Second`))
    expect(document.body.textContent).not.toContain(APPS_COPY.needsUpdate)
    expect(reports.mock.calls.flat().join(` `)).not.toContain(`INTEGRITY_ERROR`)
  })
})

describe(`AppPreview on a migrated row (round 9 §14.1)`, () => {
  it(`paints a real-shaped format 6 package, screens included`, async () => {
    const pkg = upgradePackageBody09(fixture.packages.fieldVisits) as Record<string, unknown>
    expect(pkg.formatVersion).toBe(6)
    render(
      <SidebarProvider>
        <AppPreview pkg={pkg} />
      </SidebarProvider>
    )
    await waitFor(() => expect(document.body.textContent).toContain(`Field visits`))
    expect(document.body.textContent).not.toContain(APPS_COPY.needsUpdate)
  })
})

describe(`AppSurfaceBoundary`, () => {
  const Throw = (): ReactNode => {
    throw new Error(`boom`)
  }
  it(`catches its children only: the shell's own throw still reaches the root`, () => {
    render(
      <Root>
        <AppSurfaceBoundary resetKey="a" fallback={<p>fallback</p>}>
          <p>fine</p>
        </AppSurfaceBoundary>
        <Throw />
      </Root>
    )
    expect(screen.getByTestId(`root-error`).textContent).toBe(`boom`)
  })
  it(`resets on a new key and not before`, () => {
    const view = render(
      <AppSurfaceBoundary resetKey="a" fallback={<p>fallback</p>}>
        <Throw />
      </AppSurfaceBoundary>
    )
    expect(screen.getByText(`fallback`)).toBeTruthy()
    view.rerender(
      <AppSurfaceBoundary resetKey="a" fallback={<p>fallback</p>}>
        <p>fine</p>
      </AppSurfaceBoundary>
    )
    expect(screen.getByText(`fallback`)).toBeTruthy()
    view.rerender(
      <AppSurfaceBoundary resetKey="b" fallback={<p>fallback</p>}>
        <p>fine</p>
      </AppSurfaceBoundary>
    )
    expect(screen.getByText(`fine`)).toBeTruthy()
  })
})

describe(`the guest page on a refused package`, () => {
  it(`is the dead link's page, without the validator's message`, async () => {
    vi.stubGlobal(`fetch`, vi.fn(async () => new Response(JSON.stringify({ mode: `submit`, title: `Sign up`, package: refused }), { status: 200 })))
    const Page = GuestRoute.options.component as () => ReactNode
    await act(async () => {
      render(
        <Root>
          <Page />
        </Root>
      )
    })
    await waitFor(() => expect(document.body.textContent).toContain(APP_SHARE_COPY.gone))
    expect(screen.queryByTestId(`root-error`)).toBeNull()
    expect(document.body.textContent).not.toMatch(REFUSAL)
  })
})

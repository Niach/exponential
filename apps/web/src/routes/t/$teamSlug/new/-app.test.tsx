// #EXP-2027: New › App is a PAGE; Import… reveals the import flow in the
// page (never a dialog) and an import opens the app's editor in its place.
// Lives under a `-` prefix so the route generator ignores it.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen } from "@testing-library/react"
import { APPS_TEST_IDS } from "@exp/domain-contract/apps"

const mocks = vi.hoisted(() => ({ navigate: vi.fn(), onImported: null as null | ((slug: string) => void) }))

vi.mock(`@tanstack/react-router`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  createFileRoute: () => (options: Record<string, unknown>) => ({ ...options, useParams: () => ({ teamSlug: `acme` }) }),
  useNavigate: () => mocks.navigate,
}))
vi.mock(`@/routes/t/$teamSlug/settings/-shared`, () => ({
  useSettingsPage: () => ({ team: { id: `t1`, slug: `acme` }, permissions: { isOwner: true }, resolved: true }),
}))
vi.mock(`@/hooks/use-new-app`, () => ({ useNewApp: () => ({ create: vi.fn(), pending: false }) }))
vi.mock(`@/components/apps/app-import-dialog`, () => ({
  AppImportForm: ({ layout, onImported }: { layout: string; onImported: (slug: string) => void }) => {
    mocks.onImported = onImported
    return <div data-testid="import-body" data-layout={layout} />
  },
}))

import { Route } from "./app"

const Page = (Route as unknown as { component: () => React.ReactNode }).component

beforeEach(() => {
  vi.clearAllMocks()
  mocks.onImported = null
})

describe(`New › App page`, () => {
  it(`Import… reveals the import body inline, and toggles it away`, () => {
    render(<Page />)
    expect(screen.queryByTestId(`import-body`)).toBeNull()
    fireEvent.click(screen.getByTestId(APPS_TEST_IDS.importApp))
    expect(screen.getByTestId(`import-body`).dataset.layout).toBe(`page`)
    expect(screen.getByTestId(APPS_TEST_IDS.importApp).getAttribute(`aria-expanded`)).toBe(`true`)
    fireEvent.click(screen.getByTestId(APPS_TEST_IDS.importApp))
    expect(screen.queryByTestId(`import-body`)).toBeNull()
  })

  it(`an import opens the app's editor in the page's place`, () => {
    render(<Page />)
    fireEvent.click(screen.getByTestId(APPS_TEST_IDS.importApp))
    mocks.onImported?.(`crm`)
    expect(mocks.navigate).toHaveBeenCalledWith({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug: `acme`, appSlug: `crm` }, replace: true })
  })
})

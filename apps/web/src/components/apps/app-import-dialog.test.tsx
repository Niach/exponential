// #EXP-2027: the import flow is ONE body, `AppImportForm`: the dialog (Apps
// list band, app editor) and the New › App page render it.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  validateImport: vi.fn(),
  importApp: vi.fn(),
  importTemplate: vi.fn(),
}))

vi.mock(`@tanstack/react-router`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useNavigate: () => mocks.navigate,
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    apps: {
      validateImport: { mutate: mocks.validateImport },
      import: { mutate: mocks.importApp },
      importTemplate: { mutate: mocks.importTemplate },
    },
  },
}))
vi.mock(`@/lib/collections`, () => ({
  appCollection: { utils: { awaitTxId: () => Promise.resolve() } },
}))

import { AppImportDialog, AppImportForm } from "./app-import-dialog"

beforeEach(() => {
  vi.clearAllMocks()
  mocks.validateImport.mockResolvedValue({ issues: [], upstreams: [], servers: [] })
  mocks.importApp.mockResolvedValue({ txid: 1, app: { slug: `crm` } })
})

const paste = (text: string) => fireEvent.change(screen.getByTestId(`app-import-text`), { target: { value: text } })

describe(`AppImportForm`, () => {
  it(`page layout: no dialog chrome; a valid file imports and hands back the slug`, async () => {
    const onImported = vi.fn()
    render(<AppImportForm layout="page" teamId="t1" onImported={onImported} />)
    expect(screen.getByTestId(`app-import-form`)).toBeTruthy()
    expect(screen.queryByTestId(`app-import-dialog`)).toBeNull()
    expect((screen.getByTestId(`app-import-confirm`) as HTMLButtonElement).disabled).toBe(true)
    paste(`{"app":1}`)
    await waitFor(() => expect(mocks.validateImport).toHaveBeenCalledWith({ teamId: `t1`, text: `{"app":1}` }, expect.anything()), { timeout: 2000 })
    await waitFor(() => expect((screen.getByTestId(`app-import-confirm`) as HTMLButtonElement).disabled).toBe(false))
    await act(async () => fireEvent.click(screen.getByTestId(`app-import-confirm`)))
    await waitFor(() => expect(onImported).toHaveBeenCalledWith(`crm`))
    expect(mocks.importApp).toHaveBeenCalledWith({ teamId: `t1`, text: `{"app":1}`, upstreamMappings: [] }, expect.anything())
  })

  it(`lists the server's issues and keeps Import off`, async () => {
    mocks.validateImport.mockResolvedValue({ issues: [{ path: `/name`, message: `is required` }], upstreams: [], servers: [] })
    render(<AppImportForm layout="page" teamId="t1" onImported={vi.fn()} />)
    paste(`{}`)
    await waitFor(() => expect(screen.getByTestId(`app-import-issues`).textContent).toContain(`/name: is required`), { timeout: 2000 })
    expect((screen.getByTestId(`app-import-confirm`) as HTMLButtonElement).disabled).toBe(true)
  })
})

describe(`AppImportDialog`, () => {
  it(`renders the form in the dialog; an import closes it and opens the editor`, async () => {
    const onOpenChange = vi.fn()
    render(<AppImportDialog open onOpenChange={onOpenChange} teamId="t1" teamSlug="acme" />)
    expect(screen.getByTestId(`app-import-dialog`)).toBeTruthy()
    expect(screen.queryByTestId(`app-import-form`)).toBeNull()
    paste(`{"app":1}`)
    await waitFor(() => expect((screen.getByTestId(`app-import-confirm`) as HTMLButtonElement).disabled).toBe(false), { timeout: 2000 })
    await act(async () => fireEvent.click(screen.getByTestId(`app-import-confirm`)))
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false))
    expect(mocks.navigate).toHaveBeenCalledWith({ to: `/t/$teamSlug/app-editor/$appSlug`, params: { teamSlug: `acme`, appSlug: `crm` } })
  })
})

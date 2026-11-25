// @vitest-environment jsdom
// #EXP-2140: Approve and Remove pressed on the painted surface (the press
// carries the row's id in `context`).
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { TrustRow } from "@exp/domain-contract/install-trust"

vi.mock(`@/lib/collections`, () => ({ userCollection: {} }))
vi.mock(`@tanstack/react-db`, () => ({ useLiveQuery: () => ({ data: [] }) }))
vi.mock(`@/lib/enet/use-install-trust`, () => ({ useInstallTrust: () => ({ status: `trusted`, rows: [], code: null }) }))
vi.mock(`@/lib/enet/use-key-holder`, () => ({ useTreeKeys: () => ({}) }))

const { PendingInstalls } = await import(`./install-approval`)

const row = (id: string): TrustRow => ({ kind: `newInstall`, installId: id, endpointId: id.padEnd(64, `0`), userId: `u`, label: id, code: `HNMBN A0319` })

describe(`PendingInstalls`, () => {
  let host: HTMLDivElement
  let root: Root
  beforeEach(() => {
    ;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
    host = document.body.appendChild(document.createElement(`div`))
    root = createRoot(host)
  })
  afterEach(async () => {
    await act(async () => root.unmount())
    host.remove()
  })

  it(`a press of Approve and of Remove acts on its own row`, async () => {
    const approve = vi.fn(async () => {})
    const remove = vi.fn(async () => {})
    const rows = [row(`i1`), row(`i2`)]
    await act(async () => root.render(<PendingInstalls trust={{ status: `trusted`, rows, code: null, approve, remove }} />))
    const buttons = (label: string) => [...host.querySelectorAll<HTMLElement>(`button, [role="button"]`)].filter((el) => el.textContent?.trim() === label)
    await act(async () => buttons(`Approve`)[1].click())
    expect(approve.mock.calls).toEqual([[rows[1]]])
    await act(async () => buttons(`Remove`)[0].click())
    expect(remove.mock.calls).toEqual([[rows[0]]])
  })
})

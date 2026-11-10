// #EXP-1629: Tooltip `content` is bindable (the settings MCP servers pane's
// per-row Reconnect hint reads the row's reason); the open tooltip shows the
// bound text (fixtures/bind-time.json `Tooltip/bound:content` locks the bind).

import { afterEach, describe, expect, it } from "vitest"
import { act, cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

const node: NestedNode = { id: `tip`, component: `Tooltip`, props: { content: { "@path": `/reason` } }, children: [{ id: `label`, component: `Text`, props: { text: `Reconnect` } }] }

describe(`Tooltip bound content (#EXP-1629)`, () => {
  it(`validates and shows the bound hint`, async () => {
    const reduced = reduceNested(node, { catalogId: CORE_CATALOG_ID })
    expect(reduced.issues).toEqual([])
    const { container } = render(<ExponentialSurface root={reduced.root} data={{ reason: `Token expired` }} theme="neutral" id="tb" />)
    await act(async () => {
      fireEvent.focus(container.querySelector<HTMLElement>(`[data-xui-part="Tooltip/anchor"]`)!)
    })
    expect(document.querySelector(`[data-xui-overlay="Tooltip"]`)?.textContent).toContain(`Token expired`)
  })
})

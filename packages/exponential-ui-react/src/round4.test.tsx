// Round 4 (VAPP-103) on the React renderer: the round4-contract.json
// interactions through the DOM. An Input's `change` context reads the text
// just typed (the own write lands before the context resolves), and a valid
// Form submit inside a Dialog or Drawer closes it.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential-at/ui"
import type { NestedNode } from "@exponential-at/ui"
import fixture from "@exponential-at/ui/fixtures/round4-contract.json"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

const tree = (node: unknown) => reduceNested(node as NestedNode, { catalogId: CORE_CATALOG_ID }).root

afterEach(cleanup)

describe(`round 4: the component's own write lands before its context resolves`, () => {
  for (const c of fixture.ownWrite) {
    it(c.name, () => {
      const actions: SurfaceActionEvent[] = []
      const { container } = render(<ExponentialSurface root={tree(c.input)} data={c.data} theme="neutral" id={`ow${fixture.ownWrite.indexOf(c)}`} host={{ onAction: (e) => void actions.push(e) }} />)
      const target = container.querySelector(`[data-xui-id="${c.target}"]`)!
      const input = target.querySelector(`input`)
      if (`files` in c.payload) {
        const files = (c.payload.files as { name: string; size: number; type: string }[]).map((f) => new File([`x`.repeat(f.size)], f.name, { type: f.type }))
        fireEvent.change(target.querySelector(`input[type="file"]`)!, { target: { files } })
      } else if (`value` in c.payload) fireEvent.change(input!, { target: { value: c.payload.value } })
      else fireEvent.click(target.querySelector(`button,[role="switch"]`) ?? target)
      const sent = actions.find((a) => a.name === c.expected.action.name)
      expect(sent, c.name).toBeDefined()
      // React adds the edit's `revision` to an Input's payload; the rest is the contract.
      expect(sent!.context).toMatchObject(c.expected.action.context)
    })
  }
})

describe(`round 4: a valid Form submit closes the Dialog or Drawer around it`, () => {
  for (const c of fixture.closeOnSubmit) {
    it(c.name, () => {
      const actions: SurfaceActionEvent[] = []
      const surface = (data: Record<string, unknown>) => <ExponentialSurface root={tree(c.input)} data={data} theme="neutral" id={`cs${fixture.closeOnSubmit.indexOf(c)}`} host={{ onAction: (e) => void actions.push(e) }} />
      const { container } = render(surface(c.data))
      const find = () => document.querySelector(`[data-xui-id="${c.form}-go"] button, button[data-xui-id="${c.form}-go"]`) ?? document.querySelector(`[data-xui-id="${c.form}-go"]`)
      // A Popover open at mount inside a Dialog loses to the Dialog's focus
      // (Radix: focus outside dismisses); open it again once the Dialog is
      // up, as a person would.
      if (!find()) for (const t of document.querySelectorAll<HTMLElement>(`[data-xui-id$="-trigger"] button, button[data-xui-id$="-trigger"]`)) fireEvent.click(t)
      const go = find()
      expect(go, c.name).not.toBeNull()
      fireEvent.click(go!)
      expect(actions.map((a) => a.name)).toEqual(c.actions)
      if (c.expected.overlay) expect(document.querySelector(`[data-xui-id="${c.form}"]`), `${c.expected.overlay} closed`).toBeNull()
      else expect(document.querySelector(`[data-xui-id="${c.form}"]`), `${c.name}: still open`).not.toBeNull()
      void container
    })
  }
})

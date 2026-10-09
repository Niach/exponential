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
      if (`value` in c.payload) fireEvent.change(input!, { target: { value: c.payload.value } })
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
      const { container } = render(<ExponentialSurface root={tree(c.input)} data={c.data} theme="neutral" id={`cs${fixture.closeOnSubmit.indexOf(c)}`} host={{ onAction: (e) => void actions.push(e) }} />)
      const go = container.querySelector(`[data-xui-id="${c.form}-go"] button, button[data-xui-id="${c.form}-go"]`) ?? container.querySelector(`[data-xui-id="${c.form}-go"]`)
      expect(go, c.name).not.toBeNull()
      fireEvent.click(go!)
      expect(actions.map((a) => a.name)).toEqual(c.actions)
      if (c.expected.overlay) expect(container.querySelector(`[data-xui-id="${c.form}"]`), `${c.expected.overlay} closed`).toBeNull()
      else expect(container.querySelector(`[data-xui-id="${c.form}"]`)).not.toBeNull()
    })
  }
})

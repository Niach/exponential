// EXP-1673: `inlineOverlays` (a static render) paints every overlay OPEN in
// place, no portal: server markup of each overlay specimen carries its content.

import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { CORE_CATALOG_ID, reduceNested, type NestedNode } from "@exponential_app/eui"
import specimensDoc from "@exponential_app/eui/fixtures/specimens.json"
import { ExponentialSurface } from "./surface"

const SPECIMENS = specimensDoc.specimens as unknown as { id: string; component: string; node: NestedNode }[]
const html = (id: string, inlineOverlays: boolean) => {
  const node = SPECIMENS.find((s) => s.id === id)!.node
  return renderToStaticMarkup(<ExponentialSurface id="s" root={reduceNested(node, { catalogId: CORE_CATALOG_ID }).root} inlineOverlays={inlineOverlays} mode="dark" />)
}

describe(`inlineOverlays`, () => {
  it.each([
    [`exponential-ui-dialog`, `Dialog`],
    [`exponential-ui-alert-dialog`, `Dialog`],
    [`exponential-ui-drawer`, `Drawer`],
    [`exponential-ui-popover`, `Popover`],
    [`exponential-ui-tooltip`, `Tooltip`],
    [`exponential-ui-menu`, `Menu`],
    [`exponential-ui-toast`, `Toast`],
  ])(`%s paints its %s open in server markup`, (id, overlay) => {
    expect(html(id, false)).not.toContain(`data-xui-overlay="${overlay}"`)
    const out = html(id, true)
    expect(out).toContain(`data-xui-overlays="inline"`)
    expect(out).toContain(`data-xui-overlay="${overlay}"`)
  })
})

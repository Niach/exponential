// #EDE-65 R: interactions-round1.json `composerSlots` and `composerPaste`
// on Solid, like React's interactions-fixture.test.tsx: the slots sit in
// the controls row (leading · attach · trailing · send) and stay live; a
// pasted image fires `attach` {files} and reaches onUpload.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { createSurface, type SurfaceState } from "./create-surface"
import { ExponentialSurface } from "./surface"

afterEach(() => cleanup())

type Fired = { name: string; context: Record<string, unknown> }
interface SlotsCase {
  name: string
  props: Record<string, unknown>
  slots: Record<string, NestedNode>
  direction?: `rtl`
  order: string[]
  steps: { press: string }[]
  events: Fired[]
}
interface PasteCase {
  name: string
  data: Record<string, unknown>
  clipboard: { text?: string; files?: { name: string; size: number; type: string }[] }
  events: Fired[]
  uploads: string[]
  draft: string | null
}
const fx = fixture as unknown as { composerSlots: { surface: NestedNode; cases: SlotsCase[] }; composerPaste: { surface: NestedNode; cases: PasteCase[] } }

function rowOrder(root: ParentNode): string[] {
  // #EXP-1762: a flat composer's row is its own children (the field between
  // the leading bar and the controls bar).
  const host = root.querySelector(`[data-xui-id="c"]`)!
  const flat = host.getAttribute(`data-r-layout`) === `flat`
  const row = flat ? Array.from(host.children).flatMap((el) => (el.classList.contains(`xui-composer-bar`) ? Array.from(el.children) : [el])) : Array.from(host.querySelector(`.xui-composer-bar`)!.children)
  return row.map((el) => {
    const c = el.className
    if (c.includes(`xui-Composer-field`)) return `field`
    if (c.includes(`xui-composer-leading`)) return `leading`
    if (c.includes(`xui-composer-trailing`)) return `trailing`
    if (c.includes(`xui-Composer-attachment`)) return `attachment`
    if (c.includes(`xui-Composer-send`)) return `send`
    return c
  })
}

describe(`fixtures/interactions-round1.json composerSlots`, () => {
  for (const c of fx.composerSlots.cases) {
    it(`composerSlots: ${c.name}`, () => {
      const actions: Fired[] = []
      const s = fx.composerSlots.surface
      const composer = { ...s, props: { ...s.props, ...c.props }, ...(Object.keys(c.slots).length > 0 ? { slots: c.slots } : {}) } as NestedNode
      const tree = { id: `root`, component: `Box`, style: { direction: c.direction ?? `ltr` }, children: [composer] } as NestedNode
      const { container } = render(() => <ExponentialSurface surface={createSurface({ surfaceId: `cs`, data: {}, initial: tree })} theme="neutral" id="cs" host={{ onAction: (e: Fired) => void actions.push(e) }} />)
      expect(rowOrder(container)).toEqual(c.order)
      for (const step of c.steps) fireEvent.click(container.querySelector(`[data-xui-id="${step.press}"]`)!)
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
    })
  }
})

describe(`fixtures/interactions-round1.json composerPaste`, () => {
  for (const c of fx.composerPaste.cases) {
    it(`composerPaste: ${c.name}`, () => {
      let state!: SurfaceState
      const actions: Fired[] = []
      const uploads: [string[], { name: string }][] = []
      const host = { onAction: (e: Fired) => void actions.push(e), onUpload: (files: File[], t: { name: string }) => void uploads.push([files.map((f) => f.name), t]) }
      const { container } = render(() => {
        state = createSurface({ surfaceId: `cp`, data: structuredClone(c.data), initial: fx.composerPaste.surface })
        return <ExponentialSurface surface={state} theme="neutral" id="cp" host={host} />
      })
      const field = container.querySelector(`[data-xui-id="c"] textarea`) as HTMLTextAreaElement
      const files = (c.clipboard.files ?? []).map((f) => new File([`x`.repeat(f.size)], f.name, { type: f.type }))
      const kept = fireEvent.paste(field, { clipboardData: { files, getData: () => c.clipboard.text ?? `` } })
      // jsdom inserts nothing: a paste the Composer leaves is the browser's.
      if (kept && c.clipboard.text) {
        field.value = `${field.value}${c.clipboard.text}`
        fireEvent.input(field)
      }
      expect(actions.map((a) => ({ name: a.name, context: a.context }))).toEqual(c.events)
      expect(uploads.flatMap((u) => u[0])).toEqual(c.uploads)
      for (const u of uploads) expect(u[1].name).toBe(`c`)
      expect((state.data as Record<string, unknown>).draft ?? null).toBe(c.draft)
    })
  }
})

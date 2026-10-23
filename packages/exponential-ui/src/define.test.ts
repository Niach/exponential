// VAPP-108: the typed authoring DSL compiles to the same flat list an agent
// sends, validates through the reducer, and names ids predictably.

import { describe, expect, test } from "bun:test"
import { CORE_CATALOG_ID } from "./catalog"
import { bind, call, defineSurface, el, event, flattenSurface, run, ui } from "./define"
import { reduceSurface } from "./reducer"

describe(`defineSurface`, () => {
  const devices = ui.Stack({ gap: `md` }, [
    ui.Section({ title: `Devices`, icon: `nav-devices` }),
    ui.List({ divided: true }, { template: ui.Row({ title: bind(`label`), subtitle: bind(`detail`), pressable: true }, undefined, { on: { press: run(`harness.openDevice`, { id: bind(`id`) }) } }), path: `/devices/rows`, key: `id` }),
    ui.EmptyState({ icon: `nav-devices`, title: `No devices yet`, actionLabel: `Add device` }, undefined, { visible: call(`eq`, { a: 0, b: bind(`/devices/count`) }), on: { press: event(`addDevice`) } }),
  ])

  test(`the flat list has root first, generated ids, a template child and the slots`, () => {
    const flat = flattenSurface(devices)
    expect(flat.map((c) => c.id)).toEqual([`root`, `section-1`, `list-1`, `row-1`, `emptyState-1`])
    expect(flat[0]).toEqual({ id: `root`, component: `Stack`, gap: `md`, children: [`section-1`, `list-1`, `emptyState-1`] })
    expect(flat[2].children).toEqual({ componentId: `row-1`, path: `/devices/rows`, key: `id` })
    expect(flat[3].on).toEqual({ press: { functionCall: { call: `harness.openDevice`, args: { id: { path: `id` } } } } })
    expect(flat[4].visible).toEqual({ call: `eq`, args: { a: 0, b: { path: `/devices/count` } } })
  })

  test(`a valid tree has no issues; messages are createSurface + updateComponents + the data writes`, () => {
    const surface = defineSurface(devices, { surfaceId: `devices`, data: { "/devices": { rows: [], count: 0 } } })
    expect(surface.issues).toEqual([])
    expect(surface.root.component).toBe(`Box`)
    expect(surface.messages.map((m) => Object.keys(m)[1])).toEqual([`createSurface`, `updateComponents`, `updateDataModel`])
    expect(surface.messages[0]).toEqual({ version: `v0.9`, createSurface: { surfaceId: `devices`, catalogId: CORE_CATALOG_ID } })
    expect(surface.jsonl.split(`\n`).length).toBe(3)
    // The same tree the reducer builds from the flat list an agent would send.
    expect(reduceSurface(surface.components, { catalogId: CORE_CATALOG_ID }).root).toEqual(surface.root)
  })

  test(`explicit ids win, duplicates throw, slots and extras ride along`, () => {
    const tree = ui.Card({ title: `Profile` }, [el(`Text`, { text: `Hi` }, undefined, { id: `greeting`, style: { opacity: 0.5 } })], { slots: {} })
    const dialog = ui.Dialog({ title: `Confirm` }, [ui.Text({ text: `Sure?` })], { slots: { footer: ui.Button({ label: `OK` }, undefined, { id: `ok` }) } })
    expect(flattenSurface(tree).map((c) => c.id)).toEqual([`root`, `greeting`])
    expect(flattenSurface(tree)[1].style).toEqual({ opacity: 0.5 })
    const flatDialog = flattenSurface(dialog)
    expect(flatDialog[0].slots).toEqual({ footer: `ok` })
    expect(flatDialog.map((c) => c.id)).toEqual([`root`, `text-1`, `ok`])
    expect(() => flattenSurface(ui.Stack({}, [ui.Text({ text: `a` }, undefined, { id: `x` }), ui.Text({ text: `b` }, undefined, { id: `x` })]))).toThrow(`duplicate id "x"`)
  })

  test(`a bad prop is an issue, never a throw`, () => {
    // `count` takes a literal: the agents' most common mistake (eval/report.json).
    const surface = defineSurface(ui.Section({ title: `Devices`, count: call(`len`, { value: bind(`/rows`) }) as unknown as number }))
    expect(surface.issues.map((i) => i.message)).toEqual([`props.count: expected a number`])
  })
})

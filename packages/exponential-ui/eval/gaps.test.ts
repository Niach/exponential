// VAPP-108: the rewrite gap list stays in lockstep with the view catalog:
// every app screen classified once, every named candidate macro defined,
// no stale ids; the gaps that also concern the Solid renderer are named.

import { describe, expect, test } from "bun:test"
import views from "../../view-catalog/views.json" with { type: "json" }
import gaps from "./rewrite-gaps.json" with { type: "json" }

const appViews = (views as { views: { id: string; group: string }[] }).views.filter((v) => v.group !== `exponential-ui-catalog`)

describe(`rewrite gaps`, () => {
  test(`every app screen is classified exactly once, with a reason`, () => {
    const ids = gaps.screens.map((s) => s.id)
    expect(new Set(ids).size).toBe(ids.length)
    expect(ids.sort()).toEqual(appViews.map((v) => v.id).sort())
    for (const s of gaps.screens) {
      expect(gaps.classes, s.id).toContain(s.class)
      expect(s.with.length, s.id).toBeGreaterThan(10)
    }
  })

  test(`every macro the list wants is a named candidate`, () => {
    const names = Object.keys(gaps.candidates)
    for (const s of gaps.screens.filter((s) => s.class === `macro`)) expect(names.some((n) => s.with.includes(n.split(`.`)[0])), `${s.id}: ${s.with}`).toBe(true)
  })

  test(`the gaps that also concern Solid are classes and candidates the list knows`, () => {
    const solid = gaps.renderers.solid
    for (const c of solid.classes) expect(gaps.classes).toContain(c)
    for (const name of solid.candidates) expect(Object.keys(gaps.candidates)).toContain(name)
    // Macros are the reducer's: never a renderer gap. Natives always are.
    expect(solid.classes).not.toContain(`macro`)
    expect(solid.classes).toContain(`native`)
    expect(gaps.screens.filter((s) => (solid.classes as string[]).includes(s.class)).length).toBeGreaterThan(0)
    // The element paints with Solid's painter: the same gaps, none of its own.
    expect(gaps.renderers.element.inherits).toBe(`solid`)
  })

  test(`the counts the issue quotes`, () => {
    const count = (c: string) => gaps.screens.filter((s) => s.class === c).length
    const total = gaps.screens.length
    expect(count(`expressible`) + count(`macro`) + count(`extension`) + count(`native`)).toBe(total)
    // More than half of the app is expressible today; the natives are the editor, the feed, the diff, the tree and the terminal.
    expect(count(`expressible`) * 2).toBeGreaterThan(total)
    expect(count(`native`)).toBeLessThan(total / 4)
  })
})

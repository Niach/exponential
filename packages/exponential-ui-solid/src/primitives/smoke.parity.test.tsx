// FEED-131: the jsdom half of the primitive parity pattern, proved on the
// gate's smoke specimen (plain elements; harness/primitives/smoke.tsx is
// the browser twin). A primitive lane adds one `expectElementParity` per
// specimen: React via `createElement` (its JSX stays out of this package's
// Solid transform), Solid via JSX.

import { createElement, Fragment } from "react"
import { expectElementParity, renderElementsBoth } from "../test/parity"

const BUTTON = `inline-flex h-9 items-center justify-center gap-2 rounded-md border border-input px-4 py-2 text-sm font-medium shadow-xs hover:bg-accent`
const BLOCK = `bg-primary text-primary-foreground rounded-md p-4`

const react = () => createElement(Fragment, null, createElement(`button`, { type: `button`, "data-xui-target": ``, className: BUTTON }, `Smoke button`), createElement(`div`, { className: BLOCK }, `Primary block`))
const solid = () => (
  <>
    <button type="button" data-xui-target="" class={BUTTON}>
      Smoke button
    </button>
    <div class={BLOCK}>Primary block</div>
  </>
)

describe(`primitive parity helper (smoke)`, () => {
  it(`the smoke button: one <body> on both sides`, async () => {
    await expectElementParity({ react, solid })
  })

  it(`compares the whole body: body attributes and portals count, and are cleaned up`, async () => {
    const before = document.body.outerHTML
    const portal = (owner: string) => () => {
      document.body.setAttribute(`data-scroll-locked`, owner)
      const p = document.createElement(`div`)
      p.setAttribute(`data-portal`, ``)
      document.body.appendChild(p)
    }
    const { react: r, solid: s } = await renderElementsBoth({
      react: () => (portal(`react`)(), createElement(`span`, null, `x`)),
      solid: () => (portal(`solid`)(), <span>x</span>),
    })
    expect(r).toContain(`data-scroll-locked="react"`)
    expect(s).toContain(`data-scroll-locked="solid"`)
    expect(r).toContain(`data-portal`)
    expect(r).not.toBe(s)
    expect(document.body.outerHTML).toBe(before)
  })
})

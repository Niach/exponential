// VAPP-162: QrCode and QrScanner DOM parity with React (src/test/parity.ts):
// every catalog example (each enum value × both booleans), the plate swap
// in dark mode, the empty plate, the scanner without a host camera, with a
// fake host (its feed, the scan line, a decoded value announced), a host
// that refuses, disabled, forced states, dark + rtl.

import { describe, expect, it } from "vitest"
import { act } from "react"
import type { NestedNode } from "@exponential_app/eui"
import components from "@exponential_app/eui/fixtures/catalog-components.json"
import { expectDomParity, renderReact, renderSolidSurface, serializeDom, type ParityCase } from "../test/parity"

type Case = { name: string; node: NestedNode }
const MINE = new Set([`QrCode`, `QrScanner`])
const examples = (components as { cases: Case[] }).cases.filter((c) => MINE.has(c.node.component))

const codes: NestedNode = {
  id: `root`,
  component: `Box`,
  children: [
    { id: `q1`, component: `QrCode`, props: { value: `https://exponential.at/pair/abc`, label: `Scan to pair`, size: 200 } },
    { id: `q2`, component: `QrCode`, props: { value: `x`, level: `H` } },
    { id: `q3`, component: `QrCode`, props: { value: `` } },
    { id: `q4`, component: `QrCode`, props: { value: { path: `/code` }, level: `L`, size: 96 } },
  ],
}

const scanner = (props: Record<string, unknown> = {}): NestedNode =>
  ({ id: `s`, component: `QrScanner`, props: { hint: `Point at the code`, ...props }, on: { scanned: { event: { name: `scanned` } } } }) as NestedNode

/** A fake camera: a <video> in the feed; `push` reports a decoded value. */
function fakeCamera() {
  let onValue: (v: string) => void = () => {}
  return {
    host: {
      qrScanner: {
        start: (feed: HTMLElement, _formats: readonly string[], report: (v: string) => void) => {
          feed.appendChild(document.createElement(`video`))
          onValue = report
          return () => {}
        },
      },
    },
    push: (v: string) => onValue(v),
  }
}

async function expectCameraParity(c: ParityCase, values: string[] = []): Promise<void> {
  const react = fakeCamera()
  const solid = fakeCamera()
  const r = await renderReact({ ...c, reactHost: react.host })
  const s = await renderSolidSurface({ ...c, solidHost: solid.host })
  try {
    await act(async () => {
      for (const v of values) {
        react.push(v)
        solid.push(v)
      }
      await new Promise((done) => setTimeout(done, 10))
    })
    expect(s.root.querySelector(`.xui-QrScanner-feed video`)).not.toBeNull()
    expect(serializeDom(s.root)).toBe(serializeDom(r.root))
  } finally {
    r.cleanup()
    s.cleanup()
  }
}

describe(`qr DOM parity with React`, () => {
  for (const c of examples) it(c.name, async () => expectDomParity({ tree: c.node }))
  it(`QrCode: a label, a level, the empty plate, a bound value`, async () => expectDomParity({ tree: codes, data: { code: `bound` } }))
  it(`QrCode: dark mode keeps dark modules on a light plate`, async () => expectDomParity({ tree: codes, data: { code: `bound` }, mode: `dark` }))
  it(`QrScanner without a host camera: the placeholder`, async () => expectDomParity({ tree: scanner() }))
  it(`QrScanner with a host camera: the feed and the scan line`, async () => expectCameraParity({ tree: scanner() }))
  it(`QrScanner with a host camera: a decoded value`, async () => expectCameraParity({ tree: scanner({ label: `Pair a device` }) }, [`exp://pair/1`, `exp://pair/1`]))
  it(`QrScanner: a host that refuses`, async () => expectDomParity({ tree: scanner(), reactHost: { qrScanner: { start: () => false } }, solidHost: { qrScanner: { start: () => false } } }))
  it(`QrScanner: disabled with a host`, async () => {
    const start = () => () => {}
    await expectDomParity({ tree: scanner({ disabled: true }), reactHost: { qrScanner: { start } }, solidHost: { qrScanner: { start } } })
  })
  for (const s of [`hover`, `focus`, `pressed`, `disabled`]) it(`forced state ${s}`, async () => expectDomParity({ tree: { id: `root`, component: `Box`, children: [...(codes.children ?? []), scanner()] }, data: { code: `bound` }, states: [s] }))
  it(`exponential theme, dark, rtl`, async () => expectDomParity({ tree: { id: `root`, component: `Box`, children: [...(codes.children ?? []), scanner()] }, data: { code: `bound` }, theme: `exponential`, mode: `dark`, direction: `rtl` }))
})

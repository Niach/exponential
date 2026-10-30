// VAPP-162 (docs/round-5-contract.md): the React halves of round 5 — the
// QrCode paints the core's matrix (dark on a light plate in every mode),
// the QrScanner drives the `qrScanner` host hook (placeholder without one,
// `scanned` once per distinct value, `disabled` stops), VoiceWave `bars`
// paints voice-wave.json `bars` rect for rect, the Segmented rail + count
// badge, and the serif family stays the `ui-serif` keyword.

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, qrMatrix, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import voiceWaveFixture from "@exponential_app/eui/fixtures/voice-wave.json"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

afterEach(() => cleanup())

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root

describe(`QrCode`, () => {
  it(`paints every dark module of the core's matrix in one path, inside the quiet zone`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `q`, component: `QrCode`, props: { value: `https://exponential.at`, label: `Scan to pair`, size: 200 } })} theme="neutral" id="qr" />)
    const root = container.querySelector(`[data-xui-id="q"]`)!
    expect(root.getAttribute(`role`)).toBe(`img`)
    expect(root.getAttribute(`aria-label`)).toBe(`Scan to pair`)
    const m = qrMatrix(`https://exponential.at`, `M`)!
    const svg = root.querySelector(`svg`)!
    expect(svg.getAttribute(`viewBox`)).toBe(`0 0 ${m.size + 8} ${m.size + 8}`)
    expect(svg.getAttribute(`width`)).toBe(`200`)
    const d = svg.querySelector(`path`)!.getAttribute(`d`)!
    const dark = m.rows.join(``).split(``).filter((c) => c === `1`).length
    expect([...d.matchAll(/h(\d+)v1/g)].reduce((s, r) => s + Number(r[1]), 0)).toBe(dark)
    expect(root.querySelector(`.xui-QrCode-label`)?.textContent).toBe(`Scan to pair`)
  })

  it(`keeps dark modules on a light plate in dark mode`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `q`, component: `QrCode`, props: { value: `x` } })} theme="neutral" mode="dark" id="qrd" />)
    const plate = container.querySelector(`.xui-QrCode-plate`) as HTMLElement
    const fill = container.querySelector(`.xui-QrCode-module`)!.getAttribute(`fill`)!
    const lum = (hex: string) => parseInt(hex.slice(1, 3), 16) + parseInt(hex.slice(3, 5), 16) + parseInt(hex.slice(5, 7), 16)
    const toHex = (rgb: string) => `#${(rgb.match(/\d+/g) ?? []).slice(0, 3).map((n) => Number(n).toString(16).padStart(2, `0`)).join(``)}`
    const plateHex = plate.style.backgroundColor.startsWith(`rgb`) ? toHex(plate.style.backgroundColor) : plate.style.backgroundColor
    expect(lum(plateHex)).toBeGreaterThan(lum(fill))
    // No label = decorative.
    expect(container.querySelector(`[data-xui-id="q"]`)!.getAttribute(`aria-hidden`)).toBe(`true`)
  })

  it(`an empty value is the empty plate`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `q`, component: `QrCode`, props: { value: `` } })} theme="neutral" id="qre" />)
    expect(container.querySelector(`.xui-QrCode-plate svg`)).toBeNull()
    expect(container.querySelector(`[data-xui-id="q"]`)!.getAttribute(`data-empty`)).toBe(`true`)
  })
})

describe(`QrScanner`, () => {
  const scanner = (props: Record<string, unknown> = {}) => tree({ id: `s`, component: `QrScanner`, props: { hint: `Point at the code`, ...props }, on: { scanned: { event: { name: `scanned` } } } } as NestedNode)

  it(`without a host camera: the placeholder and Camera unavailable, no scan line`, () => {
    const { container } = render(<ExponentialSurface root={scanner()} theme="neutral" id="qs" />)
    expect(container.querySelector(`.xui-QrScanner-placeholder`)?.textContent).toBe(`Camera unavailable`)
    expect(container.querySelector(`.xui-QrScanner-scanLine`)).toBeNull()
    expect(container.querySelectorAll(`.xui-QrScanner-corner`)).toHaveLength(4)
    expect(container.querySelector(`.xui-QrScanner-hint`)?.textContent).toBe(`Point at the code`)
    expect(container.querySelector(`.xui-QrScanner-viewfinder`)!.getAttribute(`aria-label`)).toBe(`Scan a QR code, Camera unavailable`)
  })

  it(`the host renders into the feed, a value fires scanned once until it changes`, () => {
    const actions: SurfaceActionEvent[] = []
    let push: (v: string) => void = () => {}
    const stop = vi.fn()
    const start = vi.fn((feed: HTMLElement, formats: readonly string[], onValue: (v: string) => void) => {
      feed.appendChild(document.createElement(`video`))
      push = onValue
      expect(formats).toEqual([`qr`])
      return stop
    })
    const { container, unmount } = render(<ExponentialSurface root={scanner()} theme="neutral" id="qsh" host={{ qrScanner: { start }, onAction: (e) => void actions.push(e) }} />)
    expect(start).toHaveBeenCalledTimes(1)
    expect(start.mock.calls[0]![0]).toBe(container.querySelector(`.xui-QrScanner-feed`))
    expect(container.querySelector(`.xui-QrScanner-feed video`)).not.toBeNull()
    expect(container.querySelector(`.xui-QrScanner-scanLine`)).not.toBeNull()
    expect(container.querySelector(`.xui-QrScanner-placeholder`)).toBeNull()
    act(() => {
      push(`exp://pair/1`)
      push(`exp://pair/1`)
      push(`exp://pair/2`)
    })
    expect(actions.map((a) => [a.event, a.payload?.value])).toEqual([
      [`scanned`, `exp://pair/1`],
      [`scanned`, `exp://pair/2`],
    ])
    unmount()
    expect(stop).toHaveBeenCalledTimes(1)
  })

  it(`disabled never starts the camera and paints the placeholder`, () => {
    const start = vi.fn(() => () => {})
    const { container } = render(<ExponentialSurface root={scanner({ disabled: true })} theme="neutral" id="qsd" host={{ qrScanner: { start } }} />)
    expect(start).not.toHaveBeenCalled()
    expect(container.querySelector(`[data-xui-id="s"]`)!.getAttribute(`aria-disabled`)).toBe(`true`)
  })

  it(`a host that refuses (false) shows the placeholder`, () => {
    const { container } = render(<ExponentialSurface root={scanner()} theme="neutral" id="qsr" host={{ qrScanner: { start: () => false } }} />)
    expect(container.querySelector(`.xui-QrScanner-placeholder`)).not.toBeNull()
  })
})

interface BarsCase {
  name: string
  input: { size: string; activity: string; level?: number; bands?: number[]; elapsed?: number; reducedMotion?: boolean }
  expected: { width: number; height: number; bars: { x: number; y: number; width: number; height: number; radius: number }[] }
}

describe(`VoiceWave variant bars (voice-wave.json bars)`, () => {
  for (const c of (voiceWaveFixture as unknown as { bars: BarsCase[] }).bars) {
    // The first frame is elapsed 0: the cases at a later time are the core's.
    if (c.input.elapsed) continue
    it(c.name, () => {
      const { container } = render(
        <ExponentialSurface root={tree({ id: `w`, component: `VoiceWave`, props: { variant: `bars`, size: c.input.size, activity: c.input.activity, level: c.input.level, bands: c.input.bands } } as NestedNode)} theme="neutral" id="vb" />
      )
      const svg = container.querySelector(`[data-xui-id="w"] svg`)!
      expect(svg.getAttribute(`viewBox`)).toBe(`0 0 ${c.expected.width} ${c.expected.height}`)
      const rects = [...svg.querySelectorAll(`rect`)]
      expect(rects.map((r) => ({ x: Number(r.getAttribute(`x`)), y: Number(r.getAttribute(`y`)), width: Number(r.getAttribute(`width`)), height: Number(r.getAttribute(`height`)), radius: Number(r.getAttribute(`rx`)) }))).toEqual(c.expected.bars)
    })
  }
  it(`a child stays in the tree, never painted`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `w`, component: `VoiceWave`, props: { variant: `bars` }, children: [{ id: `t`, component: `Text`, props: { text: `hi` } }] } as NestedNode)} theme="neutral" id="vbc" />)
    const child = container.querySelector(`.xui-VoiceWave-child`)!
    expect(child.getAttribute(`data-hidden`)).toBe(`true`)
    expect(child.getAttribute(`aria-hidden`)).toBe(`true`)
  })
})

describe(`Segmented orientation + count`, () => {
  const rail = (props: Record<string, unknown> = {}) =>
    tree({
      id: `r`,
      component: `Segmented`,
      props: {
        variant: `bar`,
        orientation: `vertical`,
        value: `chats`,
        items: [
          { label: `Chats`, value: `chats`, icon: `nav-inbox`, count: 3 },
          { label: `Calls`, value: `calls`, icon: `nav-inbox`, count: 120 },
          { label: `Settings`, value: `settings`, icon: `nav-inbox`, count: 0 },
        ],
        ...props,
      },
    } as NestedNode)

  it(`a vertical bar is the navigation rail; counts badge the icon and read after the label`, () => {
    const { container } = render(<ExponentialSurface root={rail()} theme="neutral" id="sr" />)
    const nav = container.querySelector(`nav[data-xui-id="r"]`)!
    expect(nav.getAttribute(`data-r-orientation`)).toBe(`vertical`)
    const items = [...nav.querySelectorAll(`button`)]
    expect(items.map((b) => b.querySelector(`.xui-Segmented-badge`)?.textContent ?? null)).toEqual([`3`, `99+`, null])
    expect(items.map((b) => b.getAttribute(`aria-label`))).toEqual([`Chats, 3`, `Calls, 99+`, null])
    expect(items[0]!.getAttribute(`aria-current`)).toBe(`page`)
  })

  it(`ArrowDown/ArrowUp move focus; ArrowRight does not`, () => {
    const { container } = render(<ExponentialSurface root={rail()} theme="neutral" id="srk" />)
    const items = [...container.querySelectorAll(`nav button`)] as HTMLButtonElement[]
    items[0]!.focus()
    fireEvent.keyDown(items[0]!, { key: `ArrowRight` })
    expect(document.activeElement).toBe(items[0])
    fireEvent.keyDown(items[0]!, { key: `ArrowDown` })
    expect(document.activeElement).toBe(items[1])
    fireEvent.keyDown(items[1]!, { key: `ArrowUp` })
    expect(document.activeElement).toBe(items[0])
  })

  it(`only a bar paints the badge`, () => {
    const { container } = render(<ExponentialSurface root={rail({ variant: `segmented` })} theme="neutral" id="srs" />)
    expect(container.querySelector(`.xui-Segmented-badge`)).toBeNull()
  })
})

it(`the serif family is the ui-serif keyword with its fallback`, () => {
  const { container } = render(<ExponentialSurface root={tree({ id: `t`, component: `Text`, props: { text: `Serif` }, style: { fontFamily: `$type.family.serif` } } as NestedNode)} theme="neutral" id="sf" />)
  const css = [...container.querySelectorAll(`style`)].map((s) => s.textContent).join(`\n`)
  expect(css).toContain(`ui-serif, Georgia, serif`)
  expect(css).not.toContain(`"ui-serif"`)
})

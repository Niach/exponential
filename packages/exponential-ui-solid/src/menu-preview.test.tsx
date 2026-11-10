// #EXP-1629: interactions-round1.json `menuRows` + `menuPreview` on Solid,
// like React's interactions-fixture.test.tsx: an item's `identifier` sits
// between its icon and its label, its `detail` after the label; a row's
// `preview` shows beside it while hovered (after 150 ms) or highlighted (at
// once), hides 80 ms after both left, and its inner events fire the Menu's
// `action`.

import { afterEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import type { SurfaceActionEvent } from "./host"
import { ExponentialSurface } from "./surface"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root

interface MenuRowCase {
  name: string
  items: unknown[]
  submenu: number | null
  rows: { key: string; parts: string[]; label: string; identifier?: string; detail?: string }[]
}
interface MenuPreviewStep {
  hover?: string | null
  overPreview?: boolean
  highlight?: string | null
  wait?: number
  press?: string
  shown: string | null
  texts?: string[]
  events: { name: string; context: Record<string, unknown> }[]
}
const menuRowsFixture = (fixture as unknown as { menuRows: MenuRowCase[] }).menuRows
const menuPreviewFixture = (fixture as unknown as { menuPreview: { menu: NestedNode; cases: { name: string; submenu: number | null; steps: MenuPreviewStep[] }[] } }).menuPreview

function menuRowPart(el: Element): string | null {
  const cls = (el.getAttribute(`class`) ?? ``).split(/\s+/)
  if (el.tagName.toLowerCase() === `svg`) return `icon`
  for (const [needle, part] of [[`xui-Menu-check`, `check`], [`xui-Menu-identifier`, `identifier`], [`xui-menu-label`, `label`], [`xui-Menu-detail`, `detail`], [`xui-Menu-shortcut`, `shortcut`], [`xui-Menu-submenuIndicator`, `submenuIndicator`]] as const) {
    if (cls.includes(needle)) return part
  }
  return null
}

function menuRow(key: string): HTMLElement | null {
  const path = key.split(`.`).map(Number)
  const content = document.querySelector(path.length === 1 ? `[data-xui-overlay="Menu"]` : `[data-xui-overlay="Menu.sub"]`)
  if (!content) return null
  const rows = Array.from(content.querySelectorAll(`[role^="menuitem"]`)).filter((r) => r.closest(`[data-xui-overlay]`) === content)
  return (rows[path[path.length - 1]!] as HTMLElement | undefined) ?? null
}

function openMenu(container: HTMLElement, submenu: number | null) {
  fireEvent.pointerDown(container.querySelector(`.xui-Menu-trigger`)!, { button: 0, ctrlKey: false, pointerType: `mouse` })
  if (submenu !== null) fireEvent.keyDown(menuRow(submenu.toString())!, { key: `ArrowRight` })
}

describe(`fixtures/interactions-round1.json menuRows`, () => {
  for (const c of menuRowsFixture) {
    it(`menuRows: ${c.name}`, () => {
      const { container } = render(() => <ExponentialSurface root={tree({ id: `m`, component: `Menu`, props: { label: `More`, items: c.items } } as unknown as NestedNode)} theme="neutral" id="mr" />)
      openMenu(container, c.submenu)
      for (const r of c.rows) {
        const row = menuRow(r.key)
        expect(row, r.key).not.toBeNull()
        const parts = Array.from(row!.children).map(menuRowPart).filter((p): p is string => p !== null)
        expect({ key: r.key, parts }).toEqual({ key: r.key, parts: r.parts })
        expect(row!.querySelector(`.xui-menu-label`)!.textContent).toBe(r.label)
        expect(row!.querySelector(`.xui-Menu-identifier`)?.textContent ?? null).toBe(r.identifier ?? null)
        expect(row!.querySelector(`.xui-Menu-detail`)?.textContent ?? null).toBe(r.detail ?? null)
      }
    })
  }
})

describe(`fixtures/interactions-round1.json menuPreview`, () => {
  for (const c of menuPreviewFixture.cases) {
    it(`menuPreview: ${c.name}`, () => {
      vi.useFakeTimers()
      const actions: SurfaceActionEvent[] = []
      const { container } = render(() => <ExponentialSurface root={tree(menuPreviewFixture.menu)} theme="neutral" id="mpv" host={{ onAction: (e: SurfaceActionEvent) => void actions.push(e) }} />)
      openMenu(container, c.submenu)
      let hovered: HTMLElement | null = null
      for (const [i, s] of c.steps.entries()) {
        const before = actions.length
        if (`hover` in s) {
          if (hovered) fireEvent.pointerLeave(hovered, { pointerType: `mouse` })
          hovered = s.hover ? menuRow(s.hover) : null
          if (hovered) {
            fireEvent.pointerMove(hovered, { pointerType: `mouse` })
            fireEvent.pointerEnter(hovered, { pointerType: `mouse` })
          }
        }
        if (`overPreview` in s) {
          const panel = document.querySelector(`[data-xui-menu-preview]`)!
          if (s.overPreview) fireEvent.pointerEnter(panel, { pointerType: `mouse` })
          else fireEvent.pointerLeave(panel, { pointerType: `mouse` })
        }
        if (`highlight` in s) {
          if (s.highlight) {
            const row = menuRow(s.highlight)!
            fireEvent.keyDown(document.body, { key: `Shift` })
            if (document.activeElement === row) fireEvent.keyDown(row, { key: `Shift` })
            else row.focus()
          } else (document.activeElement as HTMLElement | null)?.blur()
        }
        if (s.wait !== undefined) vi.advanceTimersByTime(s.wait)
        if (s.press) {
          const target = document.querySelector(`[data-xui-menu-preview] [data-xui-id="${s.press}"]`) as HTMLElement
          expect(target, s.press).not.toBeNull()
          fireEvent.click(target)
        }
        const panel = document.querySelector(`[data-xui-menu-preview]`)
        expect({ step: i, shown: panel?.getAttribute(`data-xui-menu-preview`) ?? null }).toEqual({ step: i, shown: s.shown })
        if (panel) {
          expect(panel.getAttribute(`data-xui-id`)).toBe(`m.preview.${s.shown}`)
          for (const t of s.texts ?? []) expect(panel.textContent, t).toContain(t)
        }
        expect(actions.slice(before).filter((a) => a.name !== `picked`).map((a) => ({ name: a.name, context: a.context }))).toEqual(s.events)
        expect(document.querySelector(`[data-xui-overlay="Menu"]`), `step ${i}: the menu stays open`).not.toBeNull()
      }
    })
  }
})

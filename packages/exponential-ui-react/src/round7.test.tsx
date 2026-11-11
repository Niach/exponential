// VAPP-276 (docs/round-7-contract.md): a Popover / Menu `anchor`ed to a node
// elsewhere in the surface (a bottom-panel button): the anchor is the
// overlay's trigger (a press toggles it, aria-haspopup / aria-expanded /
// aria-controls), `side` + `align` reach the popper, Escape closes it and
// returns focus to the anchor. Placement itself is the browser suite's
// (browser/overlay.test.ts replays overlay-geometry.json in Chromium).

import { afterEach, describe, expect, it } from "vitest"
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import type { HostedPopup } from "@exponential_app/eui/web"
import type { SurfaceActionEvent } from "./host"
import { ExponentialSurface } from "./surface"

afterEach(cleanup)

const byId = (c: Element | Document, id: string) => c.querySelector<HTMLElement>(`[data-xui-id="${id}"]`)

const panel = (): NestedNode =>
  ({
    id: `root`,
    component: `Box`,
    children: [
      {
        id: `panel`,
        component: `Box`,
        style: { display: `flex`, flexDirection: `row`, justifyContent: `space-between` },
        children: [
          { id: `start`, component: `Button`, props: { label: `Start` } },
          { id: `clock`, component: `Button`, props: { label: `12:00` } },
        ],
      },
      { id: `startMenu`, component: `Menu`, props: { anchor: `start`, side: `top`, align: `start`, items: [{ label: `Files`, value: `files` }] } },
      { id: `calendar`, component: `Popover`, props: { anchor: `clock`, side: `top`, align: `end` }, children: [{ id: `month`, component: `Text`, props: { text: `November` } }] },
    ],
  }) as unknown as NestedNode

function mount() {
  const r = reduceNested(panel(), { catalogId: CORE_CATALOG_ID })
  expect(r.issues).toEqual([])
  return render(<ExponentialSurface root={r.root} theme="neutral" id="panel" />)
}

const content = (overlay: string) => document.querySelector<HTMLElement>(`[data-xui-overlay="${overlay}"]`)

describe(`anchor by id (VAPP-276)`, () => {
  it(`the anchored node is the Popover's trigger: a press toggles it, Escape returns focus to it`, async () => {
    const { container } = mount()
    const clock = byId(container, `clock`)!
    expect(clock.getAttribute(`aria-haspopup`)).toBe(`dialog`)
    expect(clock.getAttribute(`aria-expanded`)).toBe(`false`)
    // No own trigger: the overlay lays out nothing.
    expect(byId(container, `calendar`)!.getAttribute(`data-xui-overlay-root`)).toBe(`none`)
    await act(async () => fireEvent.click(clock))
    const pop = content(`Popover`)
    expect(pop).not.toBeNull()
    expect(pop!.textContent).toContain(`November`)
    expect(pop!.getAttribute(`data-side`)).toBe(`top`)
    expect(pop!.getAttribute(`data-align`)).toBe(`end`)
    expect(clock.getAttribute(`aria-expanded`)).toBe(`true`)
    expect(clock.getAttribute(`aria-controls`)).toBe(pop!.id)
    await act(async () => fireEvent.keyDown(pop!, { key: `Escape` }))
    await waitFor(() => expect(content(`Popover`)).toBeNull())
    await waitFor(() => expect(document.activeElement).toBe(clock))
    expect(clock.getAttribute(`aria-expanded`)).toBe(`false`)
    // A second press opens it again, a third closes it.
    await act(async () => fireEvent.click(clock))
    await waitFor(() => expect(content(`Popover`)).not.toBeNull())
    await act(async () => fireEvent.click(clock))
    await waitFor(() => expect(content(`Popover`)).toBeNull())
  })

  it(`an anchored Menu draws no default trigger and opens from its anchor, side + align on the content`, async () => {
    const { container } = mount()
    expect(byId(container, `startMenu.trigger`)).toBeNull()
    const start = byId(container, `start`)!
    expect(start.getAttribute(`aria-haspopup`)).toBe(`menu`)
    await act(async () => fireEvent.click(start))
    const menu = content(`Menu`)
    expect(menu).not.toBeNull()
    expect(menu!.getAttribute(`data-side`)).toBe(`top`)
    expect(menu!.getAttribute(`data-align`)).toBe(`start`)
    expect(menu!.textContent).toContain(`Files`)
    await act(async () => fireEvent.keyDown(menu!, { key: `Escape` }))
    await waitFor(() => expect(content(`Menu`)).toBeNull())
    await waitFor(() => expect(document.activeElement).toBe(start))
  })
})

describe(`hosted popups (VAPP-281)`, () => {
  it(`host.popupHost receives the layer, the content renders through its element, presses and Escape route back`, async () => {
    const mounted: HostedPopup[] = []
    const unmounted: string[] = []
    const actions: SurfaceActionEvent[] = []
    const box = document.createElement(`div`)
    document.body.append(box)
    const tree = panel()
    // A button inside the calendar fires an action.
    ;(tree.children![2] as NestedNode).children!.push({ id: `today`, component: `Button`, props: { label: `Today` }, on: { press: { event: { name: `today` } } } } as unknown as NestedNode)
    const r = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
    expect(r.issues).toEqual([])
    const { container } = render(
      <ExponentialSurface root={r.root} theme="neutral" id="panel" host={{ onAction: (e) => void actions.push(e), popupHost: { mount: (p) => (mounted.push(p), box), unmount: (p) => void unmounted.push(p.owner) } }} />
    )
    await act(async () => fireEvent.click(byId(container, `clock`)!))
    expect(mounted.map((p) => [p.owner, p.kind, p.side, p.align])).toEqual([[`calendar`, `Popover`, `top`, `end`]])
    expect(Object.keys(mounted[0]!.anchor).sort()).toEqual([`height`, `width`, `x`, `y`])
    // The content is the host's, not the surface's overlay layer.
    const pop = content(`Popover`)!
    expect(box.contains(pop)).toBe(true)
    expect(container.querySelector(`[data-xui-layer="overlay"]`)!.contains(pop)).toBe(false)
    expect(box.getAttribute(`data-xui-hosted`)).toBe(`calendar`)
    await act(async () => fireEvent.click(byId(box, `today`)!))
    expect(actions.map((a) => a.name)).toContain(`today`)
    await act(async () => fireEvent.keyDown(pop, { key: `Escape` }))
    await waitFor(() => expect(content(`Popover`)).toBeNull())
    expect(unmounted).toEqual([`calendar`])
    await waitFor(() => expect(document.activeElement).toBe(byId(container, `clock`)))
    // A Menu goes through the host too.
    await act(async () => fireEvent.click(byId(container, `start`)!))
    expect(mounted.at(-1)!.kind).toBe(`Menu`)
    expect(box.contains(content(`Menu`))).toBe(true)
    box.remove()
  })

  it(`a host that returns null keeps the in-surface overlay`, async () => {
    const r = reduceNested(panel(), { catalogId: CORE_CATALOG_ID })
    const { container } = render(<ExponentialSurface root={r.root} theme="neutral" id="panel" host={{ popupHost: { mount: () => null } }} />)
    await act(async () => fireEvent.click(byId(container, `clock`)!))
    await waitFor(() => expect(content(`Popover`)).not.toBeNull())
    expect(container.querySelector(`[data-xui-layer="overlay"]`)!.contains(content(`Popover`))).toBe(true)
  })
})

describe(`menu item iconColor (#EXP-1629)`, () => {
  it(`tints the item's glyph: a token as its variable, the host's bound colour as is; checkbox rows keep their glyph`, async () => {
    const tree = {
      id: `root`,
      component: `Box`,
      children: [{ id: `m`, component: `Menu`, props: { label: `Status`, items: [
        { label: `In Progress`, value: `p`, kind: `checkbox`, checked: true, icon: `status-in-progress`, iconColor: `$color.warning` },
        { label: `Blocked`, value: `b`, icon: `status-in-progress`, iconColor: { "@path": `/blocked` } },
        { label: `Plain`, value: `q`, icon: `status-backlog` },
      ] } }],
    } as unknown as NestedNode
    const r = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
    expect(r.issues).toEqual([])
    const { container } = render(<ExponentialSurface root={r.root} theme="neutral" id="menu" data={{ blocked: `#ef4444` }} />)
    await act(async () => fireEvent.pointerDown(byId(container, `m.trigger`)!, { button: 0, ctrlKey: false, pointerType: `mouse` }))
    const glyphs = [...content(`Menu`)!.querySelectorAll<SVGElement>(`svg[data-icon^="status-"]`)]
    expect(glyphs.map((g) => g.style.color)).toEqual([`var(--xui-color-warning)`, `rgb(239, 68, 68)`, ``])
  })
})

// `matchAnchorWidth`: the panel takes the popper's anchor width (the anchor
// node by id, else the trigger); unset, the content keeps its own width.
describe(`Popover matchAnchorWidth`, () => {
  const tree = (matchAnchorWidth?: boolean, byIdAnchor = true): NestedNode =>
    ({
      id: `root`,
      component: `Box`,
      children: byIdAnchor
        ? [
            { id: `card`, component: `Box`, children: [{ id: `draft`, component: `Text`, props: { text: `/rev` } }] },
            { id: `commands`, component: `Popover`, props: { anchor: `card`, side: `top`, align: `start`, open: true, ...(matchAnchorWidth === undefined ? {} : { matchAnchorWidth }) }, children: [{ id: `review`, component: `Text`, props: { text: `/review` } }] },
          ]
        : [{ id: `commands`, component: `Popover`, props: { open: true, matchAnchorWidth }, slots: { trigger: { id: `t`, component: `Button`, props: { label: `Open` } } }, children: [{ id: `review`, component: `Text`, props: { text: `/review` } }] }],
    }) as unknown as NestedNode
  const open = (node: NestedNode) => {
    const r = reduceNested(node, { catalogId: CORE_CATALOG_ID })
    expect(r.issues).toEqual([])
    return render(<ExponentialSurface root={r.root} theme="neutral" id="typeahead" />)
  }
  it(`true: the content is the anchor's width (by id and a trigger)`, async () => {
    for (const byIdAnchor of [true, false]) {
      open(tree(true, byIdAnchor))
      await waitFor(() => expect(content(`Popover`)).not.toBeNull())
      expect(content(`Popover`)!.style.width, `byId ${byIdAnchor}`).toBe(`var(--radix-popover-trigger-width)`)
      cleanup()
    }
  })
  it(`false or unset: the content's own width`, async () => {
    for (const m of [false, undefined]) {
      open(tree(m))
      await waitFor(() => expect(content(`Popover`)).not.toBeNull())
      expect(content(`Popover`)!.style.width).toBe(``)
      cleanup()
    }
  })
})

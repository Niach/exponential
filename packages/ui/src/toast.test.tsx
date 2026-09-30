import { act, render, waitFor } from "@testing-library/react"
import { renderToStaticMarkup } from "react-dom/server"
import { toast as sonnerToast } from "sonner"
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/toast-stack.json"

import { conceptIcon } from "./icons.generated"
import {
  TOASTER_PROPS,
  TOAST_ACTION_CLASS,
  TOAST_CLASS,
  TOAST_CLOSE_CLASS,
  TOAST_DESCRIPTION_CLASS,
  TOAST_KIND_CONCEPT,
  TOAST_KIND_ICON_CLASS,
  TOAST_KINDS,
  TOAST_TITLE_CLASS,
  Toaster,
  ToastKindIcon,
  ToastSpecimen,
  toast,
} from "./toast"

// EXP-1031 — the web arm of packages/domain-contract/fixtures/toast-stack.json.
const { constants } = fixture

describe(`Toaster (EXP-1031 toast-stack contract)`, () => {
  it(`passes sonner the fixture's numbers`, () => {
    expect(TOASTER_PROPS.gap).toBe(constants.gap)
    expect(TOASTER_PROPS.visibleToasts).toBe(constants.visible)
    expect(TOASTER_PROPS.duration).toBe(constants.durationMs)
    expect(TOASTER_PROPS.offset).toBe(constants.viewportOffset)
    expect(TOASTER_PROPS.mobileOffset).toBe(constants.mobileViewportOffset)
    expect(TOASTER_PROPS.position).toBe(`bottom-right`)
    expect(TOASTER_PROPS.theme).toBe(`dark`)
    expect(TOASTER_PROPS.closeButton).toBe(true)
    expect(TOASTER_PROPS.toastOptions.unstyled).toBe(true)
    expect(TOASTER_PROPS.toastOptions.classNames.toast).toBe(TOAST_CLASS)
    expect(TOAST_KINDS).toEqual(constants.kinds)
  })

  it(`the card is the glass card at radius lg, fixture-wide, with no shadow`, () => {
    for (const cls of [
      `bg-glass-card-opaque`,
      `border`,
      `border-glass-stroke-card`,
      `rounded-lg`,
      `w-[${constants.width}px]`,
      `text-foreground`,
    ]) {
      expect(TOAST_CLASS.split(` `)).toContain(cls)
    }
    expect(TOAST_CLASS).not.toMatch(/shadow/)
    expect(TOAST_DESCRIPTION_CLASS).toContain(`text-muted-foreground`)
  })

  it(`the four kind icons are the concept glyphs, coloured, and only they carry colour`, () => {
    for (const kind of TOAST_KINDS) {
      expect(TOAST_KIND_CONCEPT[kind]).toBe(`ui-${kind}`)
      const glyph = renderToStaticMarkup(<ToastKindIcon kind={kind} />)
      expect(glyph).toContain(TOAST_KIND_ICON_CLASS[kind])
      const icon = TOASTER_PROPS.icons[kind] as { type: unknown; props: { kind: string } }
      expect(icon.type).toBe(ToastKindIcon)
      expect(icon.props.kind).toBe(kind)
    }
    expect(TOASTER_PROPS.icons.close.type).toBe(conceptIcon(`ui-close`))
    for (const cls of [TOAST_CLASS, TOAST_TITLE_CLASS, TOAST_DESCRIPTION_CLASS]) {
      expect(cls).not.toMatch(/(green|red|blue|yellow)-/)
    }
  })

  it(`the kind glyph is the concept icon's own svg`, () => {
    for (const kind of TOAST_KINDS) {
      const Icon = conceptIcon(TOAST_KIND_CONCEPT[kind])
      const expected = renderToStaticMarkup(<Icon />)
      const actual = renderToStaticMarkup(<ToastKindIcon kind={kind} />)
      // Same path data: strip the attributes, compare the children.
      const inner = (svg: string) => svg.replace(/^<svg[^>]*>/, ``)
      expect(inner(actual)).toBe(inner(expected))
    }
  })

  it(`re-exports sonner's toast`, () => {
    expect(toast).toBe(sonnerToast)
  })

  it(`a live toast wears the shared classes and the kind glyph`, async () => {
    render(<Toaster />)
    act(() => {
      toast.success(`Saved`, { description: `All good`, action: { label: `Undo`, onClick: () => {} } })
    })
    const card = await waitFor(() => {
      const el = document.querySelector(`[data-sonner-toast]`)
      expect(el).not.toBeNull()
      return el as HTMLElement
    })
    expect(card.className).toContain(TOAST_CLASS)
    expect(card.querySelector(`[data-toast-kind="success"]`)).not.toBeNull()
    expect(card.querySelector(`[data-close-button]`)!.className).toContain(TOAST_CLOSE_CLASS)
    expect(card.querySelector(`[data-button]`)!.className).toContain(TOAST_ACTION_CLASS)
    const list = document.querySelector(`[data-sonner-toaster]`) as HTMLElement
    expect(list.dataset.yPosition).toBe(`bottom`)
    expect(list.dataset.xPosition).toBe(`right`)
    expect(list.style.getPropertyValue(`--gap`)).toBe(`${constants.gap}px`)
    expect(list.style.getPropertyValue(`--offset-right`)).toBe(`${constants.viewportOffset}px`)
    expect(list.style.getPropertyValue(`--mobile-offset-right`)).toBe(`${constants.mobileViewportOffset}px`)
  })
})

describe(`ToastSpecimen`, () => {
  const html = (el: React.ReactElement) => {
    const host = document.createElement(`div`)
    host.innerHTML = renderToStaticMarkup(el)
    return host
  }

  it(`a single card wears the live toast's constants`, () => {
    const host = html(<ToastSpecimen kind="warning" title="Almost out" description="Two left" action="Upgrade" />)
    const card = host.querySelector(`[data-slot="toast"]`)!
    expect(card.className).toBe(TOAST_CLASS)
    expect(host.querySelector(`[data-toast-kind="warning"]`)!.getAttribute(`class`)).toContain(TOAST_KIND_ICON_CLASS.warning)
    expect(host.innerHTML).toContain(TOAST_TITLE_CLASS)
    expect(host.innerHTML).toContain(TOAST_DESCRIPTION_CLASS)
    expect(host.innerHTML).toContain(TOAST_ACTION_CLASS)
    expect(host.textContent).toContain(`Upgrade`)
  })

  // Bottom-anchored like every live placement: h = 60, peek 14, gap 14.
  const h = 60
  const expected = {
    collapsed: { height: h + 28, offsets: [0, 14, 28], scales: [0.9, 0.95, 1] },
    expanded: { height: 3 * h + 28, offsets: [0, h + 14, 2 * h + 28], scales: [1, 1, 1] },
  }
  for (const stack of [`collapsed`, `expanded`] as const) {
    it(`the ${stack} stack is bottom-anchored by the fixture's numbers`, () => {
      const want = expected[stack]
      expect(constants.peek).toBe(14)
      expect(constants.gap).toBe(14)
      const host = html(<ToastSpecimen stack={stack} />)
      const box = host.querySelector(`[data-slot="toast-specimen"]`) as HTMLElement
      expect(box.style.height).toBe(`${want.height}px`)
      const cards = [...host.querySelectorAll<HTMLElement>(`[data-slot="toast"]`)]
      expect(cards).toHaveLength(3)
      cards.forEach((card, i) => {
        expect(card.className).toContain(TOAST_CLASS)
        expect(card.style.top).toBe(`${want.offsets[i]}px`)
        expect(card.style.height).toBe(`${h}px`)
        expect(card.style.transform).toBe(`scale(${want.scales[i]})`)
        expect(card.style.transformOrigin).toBe(`bottom center`)
      })
      // The newest (front) card is last and on top.
      expect(Number(cards[2]!.style.zIndex)).toBeGreaterThan(Number(cards[0]!.style.zIndex))
    })
  }
})

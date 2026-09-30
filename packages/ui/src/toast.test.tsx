import { act, render, waitFor } from "@testing-library/react"
import { renderToStaticMarkup } from "react-dom/server"
import { toast as sonnerToast } from "sonner"
import { afterEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/toast-stack.json"

import { conceptIcon } from "./icons.generated"
import {
  TOASTER_PROPS,
  TOASTER_TOUCH_PROPS,
  TOAST_PLACEMENT,
  TOAST_TOUCH_QUERY,
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
    expect(TOASTER_PROPS.mobileOffset).toEqual({
      top: `calc(env(safe-area-inset-top, 0px) + ${constants.mobileViewportOffset}px)`,
      right: constants.mobileViewportOffset,
      bottom: constants.mobileViewportOffset,
      left: constants.mobileViewportOffset,
    })
    expect(TOASTER_PROPS.position).toBe(`bottom-right`)
    expect(TOASTER_PROPS.theme).toBe(`dark`)
    expect(TOASTER_PROPS.closeButton).toBe(true)
    expect(TOASTER_PROPS.toastOptions.unstyled).toBe(true)
    expect(TOASTER_PROPS.toastOptions.classNames.toast).toBe(`${TOAST_CLASS} origin-top`)
    expect(TOASTER_TOUCH_PROPS.toastOptions.classNames.toast).toBe(`${TOAST_CLASS} origin-bottom`)
    expect(TOAST_KINDS).toEqual(constants.kinds)
  })

  it(`places bottom-right on a pointer, top-centre on touch up to touchMaxWidth`, () => {
    expect(constants.placement).toEqual({ pointer: `bottom-right`, touch: `top-center` })
    expect(constants.touchMaxWidth).toBe(600)
    expect(TOAST_PLACEMENT).toEqual(constants.placement)
    expect(TOAST_TOUCH_QUERY).toBe(`(max-width: 600px)`)
    expect(TOASTER_PROPS.position).toBe(constants.placement.pointer)
    expect(TOASTER_TOUCH_PROPS.position).toBe(constants.placement.touch)
    // A top stack dismisses up or sideways.
    expect(TOASTER_TOUCH_PROPS.swipeDirections).toEqual([`top`, `left`, `right`])
    const { position: _p, swipeDirections: _s, toastOptions: to, ...touchRest } = TOASTER_TOUCH_PROPS
    const { position: _q, toastOptions: po, ...pointerRest } = TOASTER_PROPS
    expect(touchRest).toEqual(pointerRest)
    expect({ ...to.classNames, toast: `` }).toEqual({ ...po.classNames, toast: `` })
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

describe(`Toaster placement (matchMedia)`, () => {
  const original = window.matchMedia
  afterEach(() => {
    act(() => {
      toast.dismiss()
    })
    window.matchMedia = original
  })

  function mockMatchMedia(touch: boolean) {
    window.matchMedia = vi.fn((query: string) => ({
      matches: query === TOAST_TOUCH_QUERY ? touch : false,
      media: query,
      onchange: null,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      dispatchEvent: () => false,
    })) as never
  }

  for (const [touch, y, x] of [
    [false, `bottom`, `right`],
    [true, `top`, `center`],
  ] as const) {
    it(`renders ${y}-${x} when the touch query ${touch ? `matches` : `does not match`}`, async () => {
      mockMatchMedia(touch)
      const { unmount } = render(<Toaster />)
      act(() => {
        toast(`Placed`)
      })
      const list = await waitFor(() => {
        const el = document.querySelector(`[data-sonner-toaster]`)
        expect(el).not.toBeNull()
        return el as HTMLElement
      })
      expect(list.dataset.yPosition).toBe(y)
      expect(list.dataset.xPosition).toBe(x)
      // Back cards shrink about the edge that peeks out.
      const card = list.querySelector(`[data-sonner-toast]`) as HTMLElement
      expect(card.className.split(` `)).toContain(touch ? `origin-bottom` : `origin-top`)
      expect(window.matchMedia).toHaveBeenCalledWith(TOAST_TOUCH_QUERY)
      unmount()
    })
  }
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

  // h = 60, peek 14, gap 14. Pointer stacks are bottom-anchored; the touch
  // stack is top-anchored = the fixture's "three equal toasts collapsed from
  // the top" case (offsets 28/14/0 oldest first, height 88).
  const h = 60
  const fromTop = fixture.geometry.find((g) => g.name.startsWith(`three equal toasts collapsed from the top`))!
  const expected = {
    collapsed: { height: h + 28, offsets: [0, 14, 28], scales: [0.9, 0.95, 1], origin: `top center` },
    expanded: { height: 3 * h + 28, offsets: [0, h + 14, 2 * h + 28], scales: [1, 1, 1], origin: `top center` },
    "collapsed-touch": {
      height: fromTop.height,
      offsets: fromTop.items.map((i) => i.offset),
      scales: fromTop.items.map((i) => i.scale),
      origin: `bottom center`,
    },
  }
  it(`the touch stack reads the fixture's top-anchored case`, () => {
    expect(expected[`collapsed-touch`].offsets).toEqual([28, 14, 0])
    expect(expected[`collapsed-touch`].height).toBe(h + 28)
  })
  for (const stack of [`collapsed`, `expanded`, `collapsed-touch`] as const) {
    it(`the ${stack} stack is placed by the fixture's numbers`, () => {
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
        expect(card.style.transformOrigin).toBe(want.origin)
      })
      // The newest (front) card is last and on top.
      expect(Number(cards[2]!.style.zIndex)).toBeGreaterThan(Number(cards[0]!.style.zIndex))
    })
  }
})

// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest"
import type { EditorView } from "@tiptap/pm/view"
import {
  caretScrollDelta,
  keyboardOcclusion,
  releaseKeyboardClearance,
  revealCaretAboveKeyboard,
} from "./keyboard-caret"

describe(`keyboardOcclusion`, () => {
  it(`is the layout-viewport region below the visual viewport`, () => {
    expect(keyboardOcclusion(800, 0, 500)).toBe(300)
  })

  it(`accounts for visual-viewport panning (iOS offsetTop)`, () => {
    expect(keyboardOcclusion(800, 200, 500)).toBe(100)
  })

  it(`is zero when the keyboard is closed`, () => {
    expect(keyboardOcclusion(800, 0, 800)).toBe(0)
  })

  it(`never goes negative (resizes-content layouts)`, () => {
    expect(keyboardOcclusion(500, 0, 520)).toBe(0)
  })
})

describe(`caretScrollDelta`, () => {
  const band = { visibleTop: 0, visibleBottom: 400 }

  it(`is zero while the caret sits inside the visible band`, () => {
    expect(
      caretScrollDelta({ caretTop: 100, caretBottom: 120, ...band })
    ).toBe(0)
  })

  it(`scrolls down when the caret is behind the keyboard`, () => {
    expect(
      caretScrollDelta({
        caretTop: 500,
        caretBottom: 520,
        ...band,
        bottomMargin: 56,
      })
    ).toBe(176)
  })

  it(`keeps the bottom margin clear even when the caret is only grazing`, () => {
    expect(
      caretScrollDelta({
        caretTop: 380,
        caretBottom: 396,
        ...band,
        bottomMargin: 56,
      })
    ).toBe(52)
  })

  it(`never pushes the caret's top edge above the visible top`, () => {
    // Tiny visual viewport: revealing the full margin would overshoot.
    expect(
      caretScrollDelta({
        caretTop: 30,
        caretBottom: 120,
        visibleTop: 0,
        visibleBottom: 100,
        bottomMargin: 56,
      })
    ).toBe(30)
  })

  it(`scrolls up when the caret is above the visible band`, () => {
    expect(
      caretScrollDelta({
        caretTop: 100,
        caretBottom: 120,
        visibleTop: 200,
        visibleBottom: 600,
        topMargin: 12,
      })
    ).toBe(-112)
  })
})

describe(`revealCaretAboveKeyboard`, () => {
  const setViewports = (
    layoutHeight: number,
    visual: { offsetTop: number; height: number } | null
  ) => {
    Object.defineProperty(window, `innerHeight`, {
      configurable: true,
      value: layoutHeight,
    })
    Object.defineProperty(window, `visualViewport`, {
      configurable: true,
      value: visual,
    })
  }

  // jsdom does no layout, so emulate scroll geometry: a CLIPPING element has
  // a fixed clientHeight (padding extends its overflow); a GROWING element
  // sizes itself to its content (padding never creates overflow) — the shape
  // of an overflow-y-auto region whose flex chain never resolved a height.
  const mocked: HTMLElement[] = []
  const mockGeometry = (
    el: HTMLElement,
    opts: {
      clientHeight: number
      contentHeight: number
      grows?: boolean
      padOwner?: HTMLElement
    }
  ) => {
    mocked.push(el)
    const padPx = () =>
      Number.parseFloat((opts.padOwner ?? el).style.paddingBottom) || 0
    Object.defineProperty(el, `scrollHeight`, {
      configurable: true,
      get: () => opts.contentHeight + padPx(),
    })
    Object.defineProperty(el, `clientHeight`, {
      configurable: true,
      get: () => (opts.grows ? opts.contentHeight + padPx() : opts.clientHeight),
    })
    let scrollTop = 0
    Object.defineProperty(el, `scrollTop`, {
      configurable: true,
      get: () => scrollTop,
      set: (next: number) => {
        scrollTop = Math.max(
          0,
          Math.min(next, el.scrollHeight - el.clientHeight)
        )
      },
    })
  }

  const makeView = (caret: { top: number; bottom: number }) => {
    const scroller = document.createElement(`div`)
    scroller.style.overflowY = `auto`
    const content = document.createElement(`div`)
    scroller.appendChild(content)
    document.body.appendChild(scroller)
    const view = {
      dom: content,
      state: { selection: { head: 1 } },
      coordsAtPos: () => ({ ...caret, left: 0, right: 0 }),
    } as unknown as EditorView
    return { scroller, view }
  }

  afterEach(() => {
    releaseKeyboardClearance()
    for (const el of mocked.splice(0)) {
      // Instance mocks shadow the prototype getters — delete restores them.
      const target = el as unknown as Record<string, unknown>
      delete target.scrollHeight
      delete target.clientHeight
      delete target.scrollTop
    }
    document.body.innerHTML = ``
    document.body.style.paddingBottom = ``
    setViewports(768, null)
  })

  it(`does nothing without a visualViewport (older browsers)`, () => {
    setViewports(800, null)
    const { scroller, view } = makeView({ top: 700, bottom: 720 })
    mockGeometry(scroller, { clientHeight: 400, contentHeight: 1000 })
    revealCaretAboveKeyboard(view)
    expect(scroller.scrollTop).toBe(0)
  })

  it(`does nothing while the keyboard is closed`, () => {
    setViewports(800, { offsetTop: 0, height: 800 })
    const { scroller, view } = makeView({ top: 700, bottom: 720 })
    mockGeometry(scroller, { clientHeight: 400, contentHeight: 1000 })
    revealCaretAboveKeyboard(view)
    expect(scroller.scrollTop).toBe(0)
  })

  it(`scrolls a clipping region with room left — no padding needed`, () => {
    setViewports(800, { offsetTop: 0, height: 400 })
    const { scroller, view } = makeView({ top: 500, bottom: 520 })
    mockGeometry(scroller, { clientHeight: 400, contentHeight: 1000 })
    revealCaretAboveKeyboard(view)
    // delta = 520 - (400 - 56) = 176
    expect(scroller.scrollTop).toBe(176)
    expect(scroller.style.paddingBottom).toBe(``)
  })

  it(`extends a clipping region that ran out of scroll room`, () => {
    setViewports(800, { offsetTop: 0, height: 400 })
    const { scroller, view } = makeView({ top: 500, bottom: 520 })
    mockGeometry(scroller, { clientHeight: 400, contentHeight: 400 })
    revealCaretAboveKeyboard(view)
    expect(scroller.style.paddingBottom).toBe(`176px`)
    expect(scroller.scrollTop).toBe(176)
  })

  it(`falls through a non-clipping region to the document scroller`, () => {
    setViewports(800, { offsetTop: 0, height: 400 })
    const { scroller, view } = makeView({ top: 500, bottom: 520 })
    // The inner overflow-y-auto region grows with its content — padding it
    // can never create scroll room, so the attempt must be reverted and the
    // document (padded via <body>) must absorb the scroll instead.
    mockGeometry(scroller, { clientHeight: 700, contentHeight: 700, grows: true })
    const docEl = document.documentElement
    mockGeometry(docEl, {
      clientHeight: 800,
      contentHeight: 900,
      padOwner: document.body,
    })
    revealCaretAboveKeyboard(view)
    expect(scroller.style.paddingBottom).toBe(``)
    expect(scroller.scrollTop).toBe(0)
    // 100px of real document room + 76px created via body padding.
    expect(docEl.scrollTop).toBe(176)
    expect(document.body.style.paddingBottom).toBe(`76px`)
  })

  it(`caps clearance at occlusion + margin`, () => {
    setViewports(800, { offsetTop: 0, height: 400 })
    const { scroller, view } = makeView({ top: 780, bottom: 800 })
    mockGeometry(scroller, { clientHeight: 400, contentHeight: 400 })
    revealCaretAboveKeyboard(view)
    revealCaretAboveKeyboard(view)
    revealCaretAboveKeyboard(view)
    // occlusion (400) + bottom margin (56)
    expect(scroller.style.paddingBottom).toBe(`456px`)
  })

  it(`releases the clearance when the keyboard closes`, () => {
    setViewports(800, { offsetTop: 0, height: 400 })
    const { scroller, view } = makeView({ top: 500, bottom: 520 })
    mockGeometry(scroller, { clientHeight: 400, contentHeight: 400 })
    revealCaretAboveKeyboard(view)
    expect(scroller.style.paddingBottom).toBe(`176px`)
    setViewports(800, { offsetTop: 0, height: 800 })
    revealCaretAboveKeyboard(view)
    expect(scroller.style.paddingBottom).toBe(``)
  })

  it(`scrolls back down when the caret is above the visible band`, () => {
    setViewports(800, { offsetTop: 200, height: 400 })
    const { scroller, view } = makeView({ top: 100, bottom: 120 })
    mockGeometry(scroller, { clientHeight: 400, contentHeight: 1000 })
    scroller.scrollTop = 300
    revealCaretAboveKeyboard(view)
    // delta = 100 - (200 + 12) = -112
    expect(scroller.scrollTop).toBe(188)
  })
})

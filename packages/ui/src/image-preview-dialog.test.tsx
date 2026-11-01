import { fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { ImagePreviewDialog, PreviewMedia, previewBoxStyle } from "./image-preview-dialog"

// EXP-316/EXP-824: the shared lightbox — one dialog, three bodies. The switch
// lives in `PreviewMedia` so a host that cannot run a Radix portal (the
// styleguide's islands) still renders the specimen.

describe(`PreviewMedia`, () => {
  it(`draws an image by default`, () => {
    const { container } = render(
      <PreviewMedia src="/x.png" alt="a shot" label="x.png" />
    )
    const img = container.querySelector(`img`)
    expect(img?.getAttribute(`src`)).toBe(`/x.png`)
    expect(img?.getAttribute(`alt`)).toBe(`a shot`)
    expect(container.querySelector(`video`)).toBeNull()
  })

  it(`plays a video with its poster, and an audio clip under its name`, () => {
    const video = render(
      <PreviewMedia src="/clip.mp4" label="clip.mp4" kind="video" poster="/p.jpg" />
    )
    const node = video.container.querySelector(`video`)
    expect(node?.getAttribute(`src`)).toBe(`/clip.mp4`)
    expect(node?.getAttribute(`poster`)).toBe(`/p.jpg`)

    const audio = render(
      <PreviewMedia src="/voice.m4a" label="voice.m4a" kind="audio" />
    )
    expect(audio.container.querySelector(`audio`)?.getAttribute(`src`)).toBe(
      `/voice.m4a`
    )
    // The audio arm is the only one that names the file visibly.
    expect(audio.getByText(`voice.m4a`)).toBeTruthy()
  })
})

describe(`ImagePreviewDialog`, () => {
  it(`titles the dialog with the label, invisibly`, () => {
    render(
      <ImagePreviewDialog
        open
        onOpenChange={() => {}}
        src="/x.png"
        alt="a shot"
        label="x.png"
      />
    )
    const dialog = document.querySelector(`[role="dialog"]`)
    expect(dialog).toBeTruthy()
    expect(screen.getByText(`x.png`).className).toContain(`sr-only`)
    expect(dialog?.querySelector(`img`)?.getAttribute(`src`)).toBe(`/x.png`)
  })

  // EXP-1128 / EXP-1586: a full-page capture opens in the zoom view at the
  // reading width (fit-width at its top, the wheel scrolls it); a phone shot
  // gets a box of its own fitted size.
  it(`opens a tall picture in the zoom view at the reading width`, () => {
    render(
      <ImagePreviewDialog
        open
        onOpenChange={() => {}}
        src="/tall.jpg"
        alt="frontpage"
        label="web-mobile"
        naturalSize={{ width: 780, height: 25094 }}
      />
    )
    expect(document.querySelector(`[data-testid="preview-tall-scroll"]`)).toBeNull()
    const view = screen.getByTestId(`preview-zoom`)
    const img = view.querySelector(`img`)
    expect(img?.getAttribute(`src`)).toBe(`/tall.jpg`)
    expect(img?.getAttribute(`alt`)).toBe(`frontpage`)
  })

  it(`sizes a phone shot's box to its fitted size`, () => {
    render(
      <ImagePreviewDialog
        open
        onOpenChange={() => {}}
        src="/phone.png"
        label="ios"
        naturalSize={{ width: 1170, height: 2532 }}
      />
    )
    const view = screen.getByTestId(`preview-zoom`)
    expect(view.querySelector(`img`)?.getAttribute(`src`)).toBe(`/phone.png`)
  })

  // jsdom drops min()/calc() widths, so the box's CSS is asserted as written.
  it(`boxes a picture at its fitted size, a tall one at the reading width`, () => {
    const phone = previewBoxStyle({ width: 1170, height: 2532 })
    expect(phone.width).toBe(`min(calc(96vw - 1rem), 79rem, 1170px, calc((85vh - 1rem) * ${1170 / 2532}))`)
    expect(phone.height).toBe(`calc(${phone.width} * ${2532 / 1170})`)
    const tall = previewBoxStyle({ width: 780, height: 25094 })
    expect(tall.width).toBe(`min(calc(96vw - 1rem), 59rem, 780px)`)
    expect(tall.height).toBe(`min(calc(85vh - 1rem), calc(${tall.width} * ${25094 / 780}))`)
  })

  it(`probes an unknown size before it shows the box`, () => {
    render(
      <ImagePreviewDialog open onOpenChange={() => {}} src="/x.png" alt="a shot" label="x.png" />
    )
    expect(screen.queryByTestId(`preview-zoom`)).toBeNull()
    const probe = document.querySelector(`[role="dialog"] img`) as HTMLImageElement
    expect(probe.hidden).toBe(true)
    Object.defineProperty(probe, `naturalWidth`, { value: 1440 })
    Object.defineProperty(probe, `naturalHeight`, { value: 900 })
    fireEvent.load(probe)
    const view = screen.getByTestId(`preview-zoom`)
    expect(view.querySelector(`img`)?.style.width).toBe(`1440px`)
    expect(view.querySelector(`img`)?.getAttribute(`alt`)).toBe(`a shot`)
  })

  it(`plays a clip outside the zoom view`, () => {
    render(
      <ImagePreviewDialog open onOpenChange={() => {}} src="/clip.mp4" label="clip.mp4" kind="video" />
    )
    expect(screen.queryByTestId(`preview-zoom`)).toBeNull()
    expect(document.querySelector(`[role="dialog"] video`)?.getAttribute(`src`)).toBe(`/clip.mp4`)
  })

  it(`renders nothing while closed`, () => {
    render(
      <ImagePreviewDialog
        open={false}
        onOpenChange={() => {}}
        src="/x.png"
        label="x.png"
      />
    )
    expect(document.querySelector(`[role="dialog"]`)).toBeNull()
  })
})

// EXP-1586: the zoom keys reach the view from anywhere in the dialog, ←/→
// always page, and a phone swipe pages only while the picture is at rest.
describe(`ImagePreviewDialog zoom`, () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, `clientWidth`, { configurable: true, get: () => 800 })
    Object.defineProperty(HTMLElement.prototype, `clientHeight`, { configurable: true, get: () => 500 })
  })
  afterEach(() => {
    delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth
    delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight
  })

  // `changedTouches` on every event: the dialog's scroll lock reads it.
  const touch = (el: Element, type: `start` | `end`, points: { clientX: number; clientY: number }[]) =>
    (type === `start` ? fireEvent.touchStart : fireEvent.touchEnd)(el, {
      touches: type === `start` ? points : [],
      changedTouches: points.slice(-1),
    })
  const swipeLeft = (el: Element) => {
    touch(el, `start`, [{ clientX: 300, clientY: 200 }])
    touch(el, `end`, [{ clientX: 100, clientY: 210 }])
  }

  for (const panel of [undefined, <p key="p">Side</p>]) {
    it(`${panel ? `panel` : `bare`}: + zooms, a swipe then pans, 0 rests and the swipe pages`, () => {
      const onNext = vi.fn()
      const onPrev = vi.fn()
      render(
        <ImagePreviewDialog
          open
          onOpenChange={() => {}}
          src="/x.png"
          alt="a shot"
          label="x.png"
          naturalSize={{ width: 1440, height: 900 }}
          panel={panel}
          onNext={onNext}
          onPrev={onPrev}
        />
      )
      const dialog = document.querySelector(`[role="dialog"]`)!
      const view = screen.getByTestId(`preview-zoom`)
      const percent = () => view.querySelector(`[aria-label="Zoom level"]`)?.textContent
      expect(document.activeElement).toBe(dialog)
      expect(percent()).toBe(`56%`)
      const media = panel ? screen.getByTestId(`image-preview-media`) : dialog
      swipeLeft(media)
      expect(onNext).toHaveBeenCalledTimes(1)

      fireEvent.keyDown(dialog, { key: `+` })
      expect(percent()).not.toBe(`56%`)
      swipeLeft(media)
      expect(onNext).toHaveBeenCalledTimes(1)
      // ←/→ page even while zoomed.
      fireEvent.keyDown(dialog, { key: `ArrowLeft` })
      expect(onPrev).toHaveBeenCalledTimes(1)

      fireEvent.keyDown(dialog, { key: `0` })
      expect(percent()).toBe(`56%`)
      swipeLeft(media)
      expect(onNext).toHaveBeenCalledTimes(2)
    })
  }

  it(`never pages from a pinch`, () => {
    const onNext = vi.fn()
    render(
      <ImagePreviewDialog
        open
        onOpenChange={() => {}}
        src="/x.png"
        label="x.png"
        naturalSize={{ width: 1440, height: 900 }}
        onNext={onNext}
      />
    )
    const dialog = document.querySelector(`[role="dialog"]`)!
    touch(dialog, `start`, [{ clientX: 300, clientY: 200 }])
    touch(dialog, `start`, [{ clientX: 300, clientY: 200 }, { clientX: 400, clientY: 200 }])
    touch(dialog, `end`, [{ clientX: 100, clientY: 200 }])
    expect(onNext).not.toHaveBeenCalled()
  })

  it(`a new picture opens at rest, so the swipe pages again`, () => {
    const onNext = vi.fn()
    const props = { open: true, onOpenChange: () => {}, label: `x`, naturalSize: { width: 1440, height: 900 }, onNext }
    const { rerender } = render(<ImagePreviewDialog {...props} src="/a.png" />)
    const dialog = document.querySelector(`[role="dialog"]`)!
    fireEvent.keyDown(dialog, { key: `+` })
    swipeLeft(dialog)
    expect(onNext).not.toHaveBeenCalled()
    rerender(<ImagePreviewDialog {...props} src="/b.png" />)
    expect(screen.getByTestId(`preview-zoom`).querySelector(`[aria-label="Zoom level"]`)?.textContent).toBe(`56%`)
    swipeLeft(dialog)
    expect(onNext).toHaveBeenCalledTimes(1)
  })
})

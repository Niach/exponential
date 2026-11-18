import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/image-actions.json"
import { IMAGE_ACTIONS_COPY, IMAGE_ACTIONS_TEST_IDS, pressMenu, type ImageInput } from "@exp/domain-contract/image-actions"
import { imageMenuProps } from "@exponential_app/eui/web"

import { AppMarkdown } from "./app-markdown"
import { ImageMenuHost, imageCopyUrl, imageMenuEntries, toPngBlob } from "./image-actions"
import { Menu, MenuGestureHost, menuProps } from "./menu"
import { toast } from "./toast"
import { UserMessageBubble } from "./user-message-bubble"

// #EXP-1945: the web part of fixtures/image-actions.json: THE image menu
// (Copy image; Share on a touch browser that shares files) through THE web
// menu, the copy rule (PNG bytes, never a url, `w` dropped) and the source
// rule (another website's picture is a link, nothing fetched).

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}
globalThis.matchMedia ??= ((query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addEventListener() {},
  removeEventListener() {},
  addListener() {},
  removeListener() {},
  dispatchEvent: () => false,
})) as never

// jsdom's Blob has no arrayBuffer().
Blob.prototype.arrayBuffer ??= function arrayBuffer(this: Blob) {
  return new Promise<ArrayBuffer>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as ArrayBuffer)
    reader.onerror = () => reject(reader.error)
    reader.readAsArrayBuffer(this)
  })
}

const SRC = `/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90?w=480`
const PNG_BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13])

/** jsdom never decodes: a picture whose bytes "rendered" (or not). */
function setReady(img: HTMLElement, ready: boolean) {
  Object.defineProperty(img, `complete`, { configurable: true, get: () => ready })
  Object.defineProperty(img, `naturalWidth`, { configurable: true, get: () => (ready ? 640 : 0) })
}

let write: ReturnType<typeof vi.fn>
let fetchSpy: ReturnType<typeof vi.fn>

beforeEach(() => {
  Object.defineProperty(window, `innerWidth`, { configurable: true, value: 1024 })
  write = vi.fn(async (items: ClipboardItem[]) => {
    await (items[0] as unknown as { data: Record<string, Promise<Blob>> }).data[`image/png`]
  })
  Object.defineProperty(navigator, `clipboard`, { configurable: true, value: { write } })
  ;(globalThis as { ClipboardItem?: unknown }).ClipboardItem = class {
    data: Record<string, Promise<Blob>>
    constructor(data: Record<string, Promise<Blob>>) {
      this.data = data
    }
  }
  fetchSpy = vi.fn(async () => new Response(PNG_BYTES, { headers: { "content-type": `image/png` } }))
  vi.stubGlobal(`fetch`, fetchSpy)
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe(`the image menu's rows (menuCases)`, () => {
  for (const c of fixture.menuCases) {
    it(`${c.input}, canShareFile ${c.canShareFile}: ${c.expected.join(` + `)}`, () => {
      const rows = imageMenuEntries(SRC, c.input as ImageInput, c.canShareFile)
      expect(rows.map((row) => row.id)).toEqual(c.expected.map((item) => IMAGE_ACTIONS_TEST_IDS[item as `copyImage` | `share`]))
      expect(rows.map((row) => (row.kind === `item` ? row.label : null))).toEqual(c.expected.map((item) => IMAGE_ACTIONS_COPY[item as `copyImage` | `share`]))
    })
  }
})

describe(`the copy rule`, () => {
  it(`drops the thumbnail width, keeps every other parameter`, () => {
    expect(imageCopyUrl(SRC, `https://app.exponential.at`)).toBe(`https://app.exponential.at/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90`)
    expect(imageCopyUrl(`/api/attachments/x?w=480&token=t#f`, `https://app.exponential.at`)).toBe(`https://app.exponential.at/api/attachments/x?token=t#f`)
    expect(imageCopyUrl(`blob:https://app.exponential.at/5f0c`)).toBe(`blob:https://app.exponential.at/5f0c`)
    expect(imageCopyUrl(`data:image/png;base64,iVBORw0KGgo=`)).toBe(`data:image/png;base64,iVBORw0KGgo=`)
  })

  it(`PNG bytes pass through as they are (encodingCases passthrough)`, async () => {
    for (const c of fixture.encodingCases.filter((e) => e.expected === `passthrough`)) {
      const bytes = new Uint8Array(c.head.match(/../g)!.map((h) => Number.parseInt(h, 16)))
      const blob = new Blob([bytes], { type: `application/octet-stream` })
      const png = await toPngBlob(blob)
      expect(png.type).toBe(`image/png`)
      expect(new Uint8Array(await png.arrayBuffer())).toEqual(bytes)
    }
  })

  it(`anything else is decoded and re-encoded (a failed decode rejects)`, async () => {
    vi.stubGlobal(`createImageBitmap`, vi.fn(async () => Promise.reject(new Error(`decode`))))
    for (const c of fixture.encodingCases.filter((e) => e.expected === `reencode`)) {
      const bytes = new Uint8Array((c.head.match(/../g) ?? []).map((h) => Number.parseInt(h, 16)))
      await expect(toPngBlob(new Blob([bytes]))).rejects.toThrow()
    }
  })
})

describe(`THE image menu (right-click on a picture)`, () => {
  it(`a ready picture opens Copy image; picking it writes PNG bytes and toasts`, async () => {
    const success = vi.spyOn(toast, `success`)
    const failed = vi.spyOn(toast, `error`)
    render(
      <>
        <ImageMenuHost />
        <img data-testid="pic" alt="" src={SRC} {...imageMenuProps(SRC)} />
      </>
    )
    const img = screen.getByTestId(`pic`)
    setReady(img, true)
    expect(fireEvent.contextMenu(img, { clientX: 5, clientY: 5 })).toBe(false)
    const item = await screen.findByTestId(IMAGE_ACTIONS_TEST_IDS.copyImage)
    expect(item.textContent).toContain(IMAGE_ACTIONS_COPY.copyImage)
    // A pointer never shares.
    expect(screen.queryByTestId(IMAGE_ACTIONS_TEST_IDS.share)).toBeNull()
    act(() => item.click())
    await waitFor(() => expect(success.mock.calls.length + failed.mock.calls.length).toBeGreaterThan(0))
    expect(failed).not.toHaveBeenCalled()
    expect(success).toHaveBeenCalledWith(IMAGE_ACTIONS_COPY.copied)
    expect(write).toHaveBeenCalledTimes(1)
    const [[items]] = write.mock.calls as unknown as [[{ data: Record<string, Promise<Blob>> }[]]]
    expect(Object.keys(items[0]!.data)).toEqual([`image/png`])
    // The original, never the thumbnail.
    expect(String(fetchSpy.mock.calls[0]![0])).not.toContain(`w=`)
  })

  it(`a browser without image clipboard = THE error toast, never a url`, async () => {
    const error = vi.spyOn(toast, `error`)
    ;(globalThis as { ClipboardItem?: unknown }).ClipboardItem = undefined
    const writeText = vi.fn()
    Object.defineProperty(navigator, `clipboard`, { configurable: true, value: { writeText } })
    render(
      <>
        <ImageMenuHost />
        <img data-testid="pic" alt="" src={SRC} {...imageMenuProps(SRC)} />
      </>
    )
    const img = screen.getByTestId(`pic`)
    setReady(img, true)
    fireEvent.contextMenu(img, { clientX: 5, clientY: 5 })
    act(() => screen.getByTestId(IMAGE_ACTIONS_TEST_IDS.copyImage).click())
    await waitFor(() => expect(error).toHaveBeenCalledWith(IMAGE_ACTIONS_COPY.copyFailed))
    expect(writeText).not.toHaveBeenCalled()
  })

  it(`genUI's Markdown image is a content picture too`, async () => {
    const { container } = render(
      <>
        <ImageMenuHost />
        <AppMarkdown text={`![shot](${SRC})`} host={{ media: { baseUrl: `http://localhost`, allowPrivate: [`localhost`] } }} />
      </>
    )
    const img = container.querySelector(`[data-xui-part="Markdown/image"] > img`) as HTMLElement
    expect(img).toBeTruthy()
    setReady(img, true)
    fireEvent.contextMenu(img, { clientX: 5, clientY: 5 })
    expect(await screen.findByTestId(IMAGE_ACTIONS_TEST_IDS.copyImage)).toBeTruthy()
  })
})

describe(`the menu of the press (pressCases)`, () => {
  function Owner({ ready, onImage }: { ready: boolean; onImage: boolean }) {
    return (
      <MenuGestureHost
        menus={{
          row: ({ target, open, onOpenChange }) => (
            <Menu mode="pointer" anchor={target.anchor} open={open} onOpenChange={onOpenChange} entries={[{ kind: `item`, label: `Row menu`, onSelect: () => {} }]} />
          ),
        }}
      >
        <ImageMenuHost />
        <div data-testid="row" {...menuProps(`row`, `r1`)}>
          <span data-testid="text">text</span>
          <img
            data-testid="pic"
            alt=""
            src={SRC}
            {...imageMenuProps(SRC)}
            ref={(el) => {
              if (el) setReady(el, ready)
            }}
          />
          {onImage ? null : null}
        </div>
      </MenuGestureHost>
    )
  }

  for (const c of fixture.pressCases.filter((p) => p.ownerMenu !== null)) {
    it(`onImage ${c.onImage}, ready ${c.imageReady}, owner: ${c.expected}`, async () => {
      expect(pressMenu(c.onImage, c.imageReady, c.ownerMenu)).toBe(c.expected)
      render(<Owner ready={c.imageReady} onImage={c.onImage} />)
      fireEvent.contextMenu(screen.getByTestId(c.onImage ? `pic` : `text`), { clientX: 5, clientY: 5 })
      if (c.expected === `image`) {
        expect(await screen.findByTestId(IMAGE_ACTIONS_TEST_IDS.copyImage)).toBeTruthy()
        expect(screen.queryByText(`Row menu`)).toBeNull()
      } else {
        expect(await screen.findByText(`Row menu`)).toBeTruthy()
        expect(screen.queryByTestId(IMAGE_ACTIONS_TEST_IDS.copyImage)).toBeNull()
      }
    })
  }

  it(`a picture still loading with no owner: the browser's own menu (none)`, () => {
    render(
      <>
        <ImageMenuHost />
        <img data-testid="pic" alt="" src={SRC} {...imageMenuProps(SRC)} />
      </>
    )
    const img = screen.getByTestId(`pic`)
    setReady(img, false)
    expect(fireEvent.contextMenu(img, { clientX: 5, clientY: 5 })).toBe(true)
    expect(screen.queryByTestId(IMAGE_ACTIONS_TEST_IDS.copyImage)).toBeNull()
  })

  it(`a picture in a message bubble with its own menu: the image menu, never the bubble's`, async () => {
    render(
      <>
        <ImageMenuHost />
        <UserMessageBubble text="hi" imageSrc={SRC} menu={[{ kind: `item`, label: `Retry`, onSelect: () => {} }]} />
      </>
    )
    const img = document.querySelector(`[data-slot="user-message-thumb"]`) as HTMLElement
    setReady(img, true)
    fireEvent.contextMenu(img, { clientX: 5, clientY: 5 })
    expect(await screen.findByTestId(IMAGE_ACTIONS_TEST_IDS.copyImage)).toBeTruthy()
    expect(screen.queryByText(`Retry`)).toBeNull()
  })
})

describe(`a touch long-press`, () => {
  it(`opens Copy image + Share where the browser shares a file`, async () => {
    vi.useFakeTimers()
    Object.defineProperty(navigator, `canShare`, { configurable: true, value: () => true })
    Object.defineProperty(window, `innerWidth`, { configurable: true, value: 390 })
    try {
      render(
        <>
          <ImageMenuHost />
          <img data-testid="pic" alt="" src={SRC} {...imageMenuProps(SRC)} />
        </>
      )
      const img = screen.getByTestId(`pic`)
      setReady(img, true)
      fireEvent.pointerDown(img, { pointerType: `touch`, clientX: 5, clientY: 5 })
      act(() => void vi.advanceTimersByTime(800))
      fireEvent.pointerUp(img, { pointerType: `touch`, clientX: 5, clientY: 5 })
    } finally {
      vi.useRealTimers()
    }
    expect(await screen.findByTestId(IMAGE_ACTIONS_TEST_IDS.copyImage)).toBeTruthy()
    expect(screen.getByTestId(IMAGE_ACTIONS_TEST_IDS.share)).toBeTruthy()
    Reflect.deleteProperty(navigator, `canShare`)
  })
})

describe(`which sources load (sourceCases through THE Markdown)`, () => {
  const origin = `http://localhost`
  // The fixture's cases against this page's origin; srcs the Markdown
  // grammar cannot carry as one destination (whitespace, brackets) skip.
  const cases = fixture.sourceCases.filter((c) => c.src && !/[\s()<>\\]/.test(c.src.trim()) && !c.src.startsWith(fixture.sourcePolicy.origin) && !c.src.toLowerCase().startsWith(`https://app.exponential.at/`))
  for (const c of cases) {
    it(`${c.src} → ${c.expected.kind}`, async () => {
      const { hostImageSource } = await import(`./image-actions`)
      const { container } = render(<AppMarkdown text={`![${c.alt}](${c.src.trim()})`} host={{ media: { baseUrl: origin, allowPrivate: [`localhost`] }, imageSource: hostImageSource, mediaRequest: (src) => (hostImageSource(src, ``).kind === `load` ? { url: new URL(src, origin).href, headers: {} } : null) }} />)
      const img = container.querySelector(`img`)
      const link = container.querySelector(`a`)
      if (c.expected.kind === `load`) {
        // Never a link; an attachment or a data image loads (a blob: /
        // app-file: src is then the SDK media policy's to refuse).
        expect(link).toBeNull()
        if (!/^(blob|app-file):/i.test(c.src)) expect(img).toBeTruthy()
        return
      }
      expect(img).toBeNull()
      if (c.expected.kind === `link`) {
        const expected = c.expected as { href: string; text: string }
        // THE link path: the URL policy normalises the href, and refuses a
        // private host (then the text alone, still nothing fetched).
        if (link) expect(link.getAttribute(`href`)).toBe(new URL(expected.href).href)
        else expect(new URL(expected.href).hostname).toMatch(/^\[|^localhost$|^127\./)
        expect(container.textContent).toBe(expected.text)
      } else {
        expect(link).toBeNull()
        expect(container.textContent).toBe((c.expected as { text: string }).text)
      }
    })
  }

  it(`an external Markdown picture is a link with no <img>`, async () => {
    const { hostImageSource } = await import(`./image-actions`)
    const { container } = render(<AppMarkdown text="![A cat](https://example.com/cat.png)" host={{ imageSource: hostImageSource }} />)
    expect(container.querySelector(`img`)).toBeNull()
    const link = container.querySelector(`a`)!
    expect(link.getAttribute(`href`)).toBe(`https://example.com/cat.png`)
    expect(link.textContent).toBe(`A cat`)
  })
})

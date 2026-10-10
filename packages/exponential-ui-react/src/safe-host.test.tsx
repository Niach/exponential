// VAPP-103 (safe hosts): every href and src passes the host contract's
// policy, the markdown painter's destinations parse balanced parentheses and
// lists nest, the media limits hold, and a failed painter reaches the host
// as an A2UI RENDER_FAILED error.

import { afterEach, describe, expect, it, vi } from "vitest"
import { act, cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, ExponentialHost, MEDIA_LIMITS, MemoryTransport, reduceNested } from "@exponential-at/ui"
import type { ClientMessage, NestedNode } from "@exponential-at/ui"
import { ExponentialSurface } from "./surface"
import { HostSurface } from "./host-surface"
import { renderToStaticMarkup } from "react-dom/server"
import { BuiltinMarkdown, MAX_LINK_DEST, MAX_LINK_NESTING, linkAt, parseList, renderMarkdown } from "./markdown"
import { clearMediaCache, fetchLimited } from "./media"
import type { HostPlugin } from "./host"

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root
const paint = (node: NestedNode, host?: HostPlugin) => render(<ExponentialSurface root={tree(node)} theme="neutral" id="safe" host={host} />).container

describe(`Link: the href passes the URL policy`, () => {
  it(`a javascript: href never navigates, external or not`, () => {
    for (const external of [false, true]) {
      const c = paint({ id: `l`, component: `Link`, props: { href: `javascript:alert(1)`, label: `x`, external } })
      const a = c.querySelector(`a`)!
      expect(a.getAttribute(`href`)).toBeNull()
      expect(a.getAttribute(`target`)).toBeNull()
      expect(a.hasAttribute(`data-denied`)).toBe(true)
      cleanup()
    }
  })
  it(`the host's hosts allow-list holds for an external link`, () => {
    const c = paint({ id: `l`, component: `Link`, props: { href: `https://evil.example/`, label: `x`, external: true } }, { urls: { hosts: [`exponential.at`] } })
    expect(c.querySelector(`a`)!.getAttribute(`href`)).toBeNull()
    cleanup()
    const ok = paint({ id: `l`, component: `Link`, props: { href: `https://exponential.at/docs`, label: `x`, external: true } }, { urls: { hosts: [`exponential.at`] } })
    expect(ok.querySelector(`a`)!.getAttribute(`href`)).toBe(`https://exponential.at/docs`)
  })
})

describe(`FileUpload: a file url passes the URL policy`, () => {
  it(`a javascript: file url paints the name as text`, () => {
    const c = paint({ id: `f`, component: `FileUpload`, props: { name: `f`, files: [{ name: `evil.txt`, url: `javascript:alert(1)` }, { name: `ok.pdf`, url: `https://exponential.at/ok.pdf` }] } })
    const links = [...c.querySelectorAll(`.xui-file-list a`)].map((a) => a.getAttribute(`href`))
    expect(links).toEqual([`https://exponential.at/ok.pdf`])
    expect(c.textContent).toContain(`evil.txt`)
  })
})

describe(`Link: an external href opens through host.openUrl`, () => {
  it(`external + host.openUrl: the host opens it (no bare new tab); a denied one never reaches it`, () => {
    const openUrl = vi.fn()
    const c = paint({ id: `l`, component: `Link`, props: { href: `https://exponential.at/docs`, label: `Docs`, external: true } }, { openUrl })
    const ev = new MouseEvent(`click`, { bubbles: true, cancelable: true })
    c.querySelector(`a`)!.dispatchEvent(ev)
    expect(ev.defaultPrevented).toBe(true)
    expect(openUrl).toHaveBeenCalledWith(`https://exponential.at/docs`)
    cleanup()
    const d = paint({ id: `l`, component: `Link`, props: { href: `javascript:alert(1)`, label: `x`, external: true } }, { openUrl })
    fireEvent.click(d.querySelector(`a`)!)
    expect(openUrl).toHaveBeenCalledTimes(1)
  })
})

describe(`openUrl without a host opener: the policy still applies`, () => {
  it(`opens https in a new tab, never javascript:`, () => {
    const open = vi.spyOn(window, `open`).mockImplementation(() => null)
    const button = (url: string): NestedNode => ({ id: `b`, component: `Button`, props: { label: `Go` }, on: { press: { functionCall: { call: `openUrl`, args: { url } } } } })
    fireEvent.click(paint(button(`javascript:alert(1)`)).querySelector(`button`)!)
    expect(open).not.toHaveBeenCalled()
    cleanup()
    fireEvent.click(paint(button(`https://exponential.at/x`)).querySelector(`button`)!)
    expect(open).toHaveBeenCalledWith(`https://exponential.at/x`, `_blank`, `noopener,noreferrer`)
  })
})

describe(`media: every src passes the media policy`, () => {
  it(`a file:// or javascript: Image src never loads; the fallback paints`, () => {
    for (const src of [`file:///etc/passwd`, `javascript:alert(1)`]) {
      const c = paint({ id: `i`, component: `Image`, props: { src, alt: `x` } })
      expect(c.querySelector(`img`)).toBeNull()
      expect(c.querySelector(`[data-xui-c="Image"]`)!.getAttribute(`data-state`)).toBe(`error`)
      cleanup()
    }
  })
  it(`a host that lists file may load one`, () => {
    const c = paint({ id: `i`, component: `Image`, props: { src: `file:///tmp/a.png`, alt: `x` } }, { media: { schemes: [`file`] } })
    expect(c.querySelector(`img`)!.getAttribute(`src`)).toBe(`file:///tmp/a.png`)
  })
  it(`a host mediaRequest cannot widen the schemes`, () => {
    const c = paint({ id: `i`, component: `Image`, props: { src: `x`, alt: `x` } }, { mediaRequest: () => ({ url: `file:///etc/passwd`, headers: {} }) })
    expect(c.querySelector(`img`)).toBeNull()
  })
})

describe(`markdown`, () => {
  it(`destinations parse balanced parentheses (CommonMark)`, () => {
    expect(linkAt(`[a](https://x/A_(b))`, 0)).toEqual({ label: `a`, dest: `https://x/A_(b)`, end: 20 })
    expect(linkAt(`[a](a(b))`, 0)?.dest).toBe(`a(b)`)
    expect(linkAt(`[i](javascript:alert(3))`, 0)?.dest).toBe(`javascript:alert(3)`)
    expect(linkAt(`[a](x "title")`, 0)?.dest).toBe(`x`)
    expect(linkAt(`[a](<x y>)`, 0)?.dest).toBe(`x y`)
    expect(linkAt(`[a](x y)`, 0)).toBeNull()
    expect(linkAt(`[a](x(`, 0)).toBeNull()
  })
  it(`a denied link is its label as text; an inline image its alt (×4)`, () => {
    const { container } = render(<div>{renderMarkdown(`see [docs](https://en.wikipedia.org/wiki/A_(b)) and [bad](javascript:alert(1)) ![pic](https://exponential.at/p.png) end`)}</div>)
    const hrefs = [...container.querySelectorAll(`a`)].map((a) => a.getAttribute(`href`))
    expect(hrefs).toEqual([`https://en.wikipedia.org/wiki/A_(b)`])
    expect(container.querySelector(`img`)).toBeNull()
    expect(container.textContent).toContain(`pic end`)
    expect(container.textContent).toContain(`bad`)
    expect(container.textContent).toContain(` end`)
    expect(container.innerHTML).not.toContain(`javascript`)
  })
  it(`a paragraph that is one image is a block image through the media policy`, () => {
    const { container } = render(<div>{renderMarkdown(`![chart](https://exponential.at/c.png)`)}</div>)
    expect(container.querySelector(`.xui-Markdown-image img`)!.getAttribute(`src`)).toBe(`https://exponential.at/c.png`)
    expect(container.querySelector(`.xui-Markdown-image`)!.getAttribute(`aria-label`)).toBe(`chart`)
  })
  it(`a denied block image is a paragraph of its alt text, nothing without one (×4)`, () => {
    const { container } = render(<div>{renderMarkdown(`![pic](javascript:alert(3))\n\n![](javascript:x)`)}</div>)
    expect(container.querySelector(`.xui-Markdown-image`)).toBeNull()
    expect([...container.querySelectorAll(`.xui-Markdown-paragraph`)].map((p) => p.textContent)).toEqual([`pic`])
    expect(container.innerHTML).not.toContain(`javascript`)
  })
  it(`link parsing is linear: 128 KB of unclosed brackets / parens / quotes parses fast`, () => {
    const N = 128 * 1024
    for (const t of [`[a](`.repeat(N / 4), `[`.repeat(N), `[a](<`.repeat(N / 5), `[x](y z `.repeat(N / 8), `[a](` + `(`.repeat(N), `[a](b) `.repeat(N / 7) + "`c`", `>`.repeat(N)]) {
      const t0 = performance.now()
      renderToStaticMarkup(<BuiltinMarkdown text={t} />)
      expect(performance.now() - t0).toBeLessThan(1500)
    }
  })
  it(`link labels nest at most MAX_LINK_NESTING deep; a long destination is no link`, () => {
    let nest = `x`
    for (let k = 0; k < 40; k++) nest = `[${nest}](https://e.com/${k})`
    expect(renderToStaticMarkup(<BuiltinMarkdown text={nest} />).match(/<a /g)).toHaveLength(MAX_LINK_NESTING)
    expect(linkAt(`[a](https://e.com/${`x`.repeat(MAX_LINK_DEST)})`, 0)).toBeNull()
    expect(linkAt(`[a](https://e.com/${`x`.repeat(100)})`, 0)?.dest).toHaveLength(114)
    expect(linkAt(`\\[a](b)`, 1)).toBeNull()
  })
  it(`a markdown link opens through host.openUrl when the host has one`, () => {
    const openUrl = vi.fn()
    const { container } = render(<BuiltinMarkdown text={`[docs](https://exponential.at/d)`} host={{ openUrl }} />)
    fireEvent.click(container.querySelector(`a`)!)
    expect(openUrl).toHaveBeenCalledWith(`https://exponential.at/d`)
  })
  it(`lists nest by indentation`, () => {
    const list = parseList([`- a`, `  - a1`, `    1. deep`, `  - a2`, `- b`, `- [x] done`])
    expect(list.items.map((i) => i.text)).toEqual([`a`, `b`, `done`])
    expect(list.items[0]!.children[0]!.items.map((i) => i.text)).toEqual([`a1`, `a2`])
    expect(list.items[0]!.children[0]!.items[0]!.children[0]).toMatchObject({ ordered: true, items: [{ text: `deep` }] })
    expect(list.items[2]!.task).toBe(true)
    const { container } = render(<div>{renderMarkdown(`- a\n  - a1\n- b`)}</div>)
    expect(container.querySelectorAll(`ul ul li`)).toHaveLength(1)
    expect(container.querySelectorAll(`ul > li`)).toHaveLength(3)
  })
})

describe(`media limits`, () => {
  const limits = { maxBytes: 100, timeoutMs: 1000, maxPixels: 1000 }
  it(`a Content-Length over maxBytes is refused before the body`, async () => {
    vi.spyOn(globalThis, `fetch`).mockResolvedValue(new Response(`x`, { status: 200, headers: { "content-length": `5000` } }))
    await expect(fetchLimited(`https://x.test/a`, {}, limits)).rejects.toThrow(/over 100 bytes/)
  })
  it(`a streamed body over maxBytes is refused`, async () => {
    vi.spyOn(globalThis, `fetch`).mockResolvedValue(new Response(new Uint8Array(500), { status: 200 }))
    await expect(fetchLimited(`https://x.test/a`, {}, limits)).rejects.toThrow(/over 100 bytes/)
  })
  it(`an image header over maxPixels is refused before decoding`, async () => {
    const png = new Uint8Array(24)
    png.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52])
    new DataView(png.buffer).setUint32(16, 40_000)
    new DataView(png.buffer).setUint32(20, 30_000)
    vi.spyOn(globalThis, `fetch`).mockResolvedValue(new Response(png, { status: 200 }))
    await expect(fetchLimited(`https://x.test/a.png`, {}, { ...limits, maxBytes: 1000 })).rejects.toThrow(/40000×30000/)
  })
  it(`Video/Audio with headers stream past the byte caps; the poster keeps the image limits`, async () => {
    clearMediaCache()
    let n = 0
    vi.spyOn(URL, `createObjectURL`).mockImplementation(() => `blob:media-${++n}`)
    vi.spyOn(URL, `revokeObjectURL`).mockImplementation(() => {})
    // Over the 20 MB image cap, as an authed video is.
    vi.spyOn(globalThis, `fetch`).mockImplementation(async () => new Response(new Uint8Array(8), { status: 200, headers: { "content-length": String(MEDIA_LIMITS.maxBytes + 1) } }))
    const host: HostPlugin = { mediaRequest: (src) => ({ url: src, headers: { authorization: `Bearer t` } }) }
    const c = paint({ id: `v`, component: `Video`, props: { src: `https://exponential.at/clip.mp4`, poster: `https://exponential.at/poster.png` } }, host)
    await vi.waitFor(() => expect(c.querySelector(`video`)!.getAttribute(`src`)).toMatch(/^blob:media-/))
    expect(c.querySelector(`video`)!.getAttribute(`poster`)).toBeNull()
    cleanup()
    const a = paint({ id: `a`, component: `AudioPlayer`, props: { src: `https://exponential.at/talk.m4a` } }, host)
    await vi.waitFor(() => expect(a.querySelector(`audio`)!.getAttribute(`src`)).toMatch(/^blob:media-/))
  })
  it(`a host mediaStreamUrl hands the player a header-less url, re-checked by the policy`, async () => {
    const fetchSpy = vi.spyOn(globalThis, `fetch`)
    const streamed: string[] = []
    const host: HostPlugin = {
      mediaRequest: (src) => ({ url: src, headers: { authorization: `Bearer t` } }),
      mediaStreamUrl: async (req) => (streamed.push(req.url), req.url.endsWith(`evil.mp4`) ? `javascript:alert(1)` : `${req.url}?sig=abc`),
    }
    const c = paint({ id: `v`, component: `Video`, props: { src: `https://exponential.at/clip.mp4` } }, host)
    await vi.waitFor(() => expect(c.querySelector(`video`)!.getAttribute(`src`)).toBe(`https://exponential.at/clip.mp4?sig=abc`))
    expect(streamed).toEqual([`https://exponential.at/clip.mp4`])
    expect(fetchSpy).not.toHaveBeenCalled()
    cleanup()
    const d = paint({ id: `v`, component: `Video`, props: { src: `https://exponential.at/evil.mp4` } }, host)
    await new Promise((r) => setTimeout(r, 10))
    expect(d.querySelector(`video`)!.getAttribute(`src`)).toBeNull()
  })
  it(`a request past timeoutMs is aborted`, async () => {
    vi.spyOn(globalThis, `fetch`).mockImplementation((_u, init) => new Promise((_, reject) => (init?.signal as AbortSignal).addEventListener(`abort`, () => reject(new Error(`aborted`)))))
    await expect(fetchLimited(`https://x.test/slow`, {}, { ...limits, timeoutMs: 10 })).rejects.toThrow(/aborted/)
  })
})

describe(`paint failures reach the host (onPaintError → RENDER_FAILED)`, () => {
  it(`a throwing painter sends ONE A2UI error with the component's path`, () => {
    vi.spyOn(console, `error`).mockImplementation(() => {})
    const transport = new MemoryTransport()
    const host = new ExponentialHost({ transport })
    const Broken = () => {
      throw new Error(`boom`)
    }
    const seen: string[] = []
    render(<HostSurface host={host} surfaceId="s" theme="neutral" plugin={{ Markdown: Broken, onPaintError: (e) => void seen.push(e.componentId) }} />)
    act(() => host.connect())
    act(() =>
      transport.feed(
        { version: `v0.9`, createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG_ID } },
        { version: `v0.9`, updateComponents: { surfaceId: `s`, components: [{ id: `root`, component: `Box`, children: [`md`, `t`] }, { id: `md`, component: `Markdown`, text: `x` }, { id: `t`, component: `Text`, text: `still here` }] } }
      )
    )
    const errors = (transport.sent as ClientMessage[]).filter((m) => `error` in m).map((m) => (m as Extract<ClientMessage, { error: unknown }>).error)
    expect(errors).toEqual([{ code: `RENDER_FAILED`, surfaceId: `s`, message: `Markdown failed to paint: boom`, path: `/components/md` }])
    expect(seen).toEqual([`md`])
    expect(host.issues.at(-1)).toMatchObject({ code: `RENDER_FAILED`, surfaceId: `s` })
    host.paintError({ surfaceId: `s`, componentId: `md`, message: `Markdown failed to paint: boom` })
    expect((transport.sent as ClientMessage[]).filter((m) => `error` in m)).toHaveLength(1)
  })
  it(`a Select with options: [null] paints without throwing`, () => {
    const seen: unknown[] = []
    const c = paint({ id: `s`, component: `Select`, props: { label: `Pick`, name: `p`, options: [null, { label: `A`, value: `a` }, 3] } }, { onPaintError: (e) => void seen.push(e) })
    expect(seen).toEqual([])
    expect(c.querySelector(`[data-xui-error]`)).toBeNull()
  })
})

// FEED-128 (React's safe-host.test.tsx, the media half; VAPP-103 safe
// hosts) on Solid: every media src passes the host contract's policy (a
// denied one loads nothing), the media limits hold (an oversized image is
// refused, streams pass the byte caps, a stream url is re-checked), and
// markdown images go through the same policy.

import { afterEach, describe, expect, it, vi } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, MEDIA_LIMITS, reduceNested } from "@exponential-at/ui"
import type { NestedNode } from "@exponential-at/ui"
import { ExponentialSurface } from "./surface"
import { renderMarkdown } from "./markdown"
import { clearMediaCache, fetchLimited } from "./media"
import type { HostPlugin } from "./host"

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root
const paint = (node: NestedNode, host?: HostPlugin) => render(() => <ExponentialSurface root={tree(node)} theme="neutral" id="safe" host={host} />).container
const markdown = (text: string) => render(() => <div>{renderMarkdown(text)}</div>).container

describe(`media: every src passes the media policy`, () => {
  it(`a file:// or javascript: Image src never loads; the fallback paints`, () => {
    for (const src of [`file:///etc/passwd`, `javascript:alert(1)`]) {
      const c = paint({ id: `i`, component: `Image`, props: { src, alt: `x` } })
      expect(c.querySelector(`img`)).toBeNull()
      expect(c.querySelector(`[data-xui-c="Image"]`)!.getAttribute(`data-state`)).toBe(`error`)
      expect(c.querySelector(`.xui-Image-fallback`)).not.toBeNull()
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
  it(`a denied Video / AudioPlayer / Avatar src and a denied poster load nothing`, () => {
    const c = paint({
      id: `s`,
      component: `Stack`,
      children: [
        { id: `v`, component: `Video`, props: { src: `javascript:alert(1)`, poster: `file:///etc/passwd` } },
        { id: `a`, component: `AudioPlayer`, props: { src: `file:///etc/passwd` } },
        { id: `av`, component: `Avatar`, props: { name: `Jane Doe`, src: `javascript:alert(1)` } },
      ],
    } as unknown as NestedNode)
    expect(c.querySelector(`video`)!.hasAttribute(`src`)).toBe(false)
    expect(c.querySelector(`video`)!.hasAttribute(`poster`)).toBe(false)
    expect(c.querySelector(`audio`)!.hasAttribute(`src`)).toBe(false)
    expect(c.querySelector(`.xui-Avatar-image`)).toBeNull()
    expect(c.innerHTML).not.toContain(`javascript`)
    expect(c.innerHTML).not.toContain(`file://`)
  })
  it(`an Image whose host fetch fails (or is oversized) shows the fallback and the alt`, async () => {
    clearMediaCache()
    vi.spyOn(globalThis, `fetch`).mockResolvedValue(new Response(`x`, { status: 200, headers: { "content-length": String(MEDIA_LIMITS.maxBytes + 1) } }))
    const host: HostPlugin = { mediaRequest: (src) => ({ url: `https://x.test/${src}`, headers: { authorization: `Bearer t` } }) }
    const c = paint({ id: `i`, component: `Image`, props: { src: `huge.png`, alt: `Huge` } }, host)
    const root = c.querySelector(`[data-xui-id="i"]`)!
    expect(root.getAttribute(`data-state`)).toBe(`loading`)
    await vi.waitFor(() => expect(root.getAttribute(`data-state`)).toBe(`error`))
    expect(root.getAttribute(`aria-label`)).toBe(`Huge`)
    expect(c.querySelector(`.xui-Image-fallback`)).not.toBeNull()
    expect(c.querySelector(`img`)).toBeNull()
  })
})

describe(`markdown images go through the media policy`, () => {
  it(`a denied inline image is its alt; a denied link its label`, () => {
    const c = markdown(`see [bad](javascript:alert(1)) ![pic](javascript:alert(2)) end`)
    expect(c.querySelector(`img`)).toBeNull()
    expect(c.querySelector(`a`)).toBeNull()
    expect(c.textContent).toContain(`pic end`)
    expect(c.innerHTML).not.toContain(`javascript`)
  })
  it(`a paragraph that is one image is a block image through the media policy`, () => {
    const c = markdown(`![chart](https://exponential.at/c.png)`)
    expect(c.querySelector(`.xui-Markdown-image img`)!.getAttribute(`src`)).toBe(`https://exponential.at/c.png`)
    expect(c.querySelector(`.xui-Markdown-image`)!.getAttribute(`aria-label`)).toBe(`chart`)
  })
  it(`a denied block image is a paragraph of its alt text, nothing without one`, () => {
    const c = markdown(`![pic](javascript:alert(3))\n\n![](javascript:x)`)
    expect(c.querySelector(`.xui-Markdown-image`)).toBeNull()
    expect([...c.querySelectorAll(`.xui-Markdown-paragraph`)].map((p) => p.textContent)).toEqual([`pic`])
    expect(c.innerHTML).not.toContain(`javascript`)
  })
})

describe(`media limits`, () => {
  const limits = { maxBytes: 100, timeoutMs: 1000, maxPixels: 1000 }
  it(`a Content-Length over maxBytes is refused before the body`, async () => {
    vi.spyOn(globalThis, `fetch`).mockResolvedValue(new Response(`x`, { status: 200, headers: { "content-length": `5000` } }))
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
})

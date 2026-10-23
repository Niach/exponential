// FEED-128 (React's safe-host.test.tsx, the media half; VAPP-103 safe
// hosts) on Solid: every media src passes the host contract's policy (a
// denied one loads nothing), the media limits hold (an oversized image is
// refused, streams pass the byte caps, a stream url is re-checked), and
// markdown images go through the same policy. VAPP-107 on Solid: a private,
// loopback or link-local host is denied unless the host opts it in (the
// page's own private origin is), `schemes: ["https"]` is https only for
// hrefs and srcs alike, and the video/audio whole-fetch fallback keeps the
// byte cap and a total timeout. The filter is the shared web layer's
// (`@exponential_app/ui/web` urls.ts / media.ts): one code path, both
// renderers, locked here through the Solid natives.

import { afterEach, describe, expect, it, vi } from "vitest"
import { cleanup, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, MEDIA_LIMITS, reduceNested } from "@exponential_app/ui"
import type { NestedNode } from "@exponential_app/ui"
import { ExponentialSurface } from "./surface"
import { renderMarkdown } from "./markdown"
import { clearMediaCache, fetchLimited, fetchStream } from "./media"
import type { HostPlugin } from "./host"

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root
const paint = (node: NestedNode, host?: HostPlugin) => render(() => <ExponentialSurface root={tree(node)} theme="neutral" id="safe" host={host} />).container
const markdown = (text: string) => render(() => <div>{renderMarkdown(text)}</div>).container

describe(`VAPP-107: a private host is denied unless the host opts it in`, () => {
  it(`a LAN camera src loads nothing; a Link to a loopback literal paints as text`, () => {
    const c = paint({ id: `i`, component: `Image`, props: { src: `http://192.168.1.10/cam.jpg`, alt: `cam` } })
    expect(c.querySelector(`img`)?.getAttribute(`src`) ?? null).toBeNull()
    expect(c.querySelector(`[data-xui-c="Image"]`)!.getAttribute(`data-state`)).toBe(`error`)
    cleanup()
    const l = paint({ id: `l`, component: `Link`, props: { href: `http://127.0.0.1:3000/x`, label: `x`, external: true } })
    expect(l.querySelector(`a`)!.getAttribute(`href`)).toBeNull()
    expect(l.querySelector(`a`)!.hasAttribute(`data-denied`)).toBe(true)
  })
  it(`the cloud metadata endpoint, a .local name and an IPv4-mapped loopback are private too`, () => {
    for (const src of [`http://169.254.169.254/latest/meta-data/`, `http://printer.local/a.png`, `http://[::ffff:127.0.0.1]/a.png`, `http://2130706433/a.png`]) {
      const c = paint({ id: `i`, component: `Image`, props: { src, alt: `a` } })
      expect(c.querySelector(`img`)?.getAttribute(`src`) ?? null).toBeNull()
      cleanup()
    }
  })
  it(`a host that opts 10.0.0.5 in loads from it, and only from it`, () => {
    const host: HostPlugin = { media: { allowPrivate: [`10.0.0.5`] }, urls: { allowPrivate: [`10.0.0.5`] } }
    const c = paint({ id: `i`, component: `Image`, props: { src: `http://10.0.0.5/a.png`, alt: `a` } }, host)
    expect(c.querySelector(`img`)!.getAttribute(`src`)).toBe(`http://10.0.0.5/a.png`)
    cleanup()
    const lan = paint({ id: `i`, component: `Image`, props: { src: `http://10.0.0.6/a.png`, alt: `a` } }, host)
    expect(lan.querySelector(`img`)?.getAttribute(`src`) ?? null).toBeNull()
    cleanup()
    const l = paint({ id: `l`, component: `Link`, props: { href: `http://10.0.0.5/x`, label: `x`, external: true } }, host)
    expect(l.querySelector(`a`)!.getAttribute(`href`)).toBe(`http://10.0.0.5/x`)
  })
  it(`a hosts allowlist alone does not opt a private host in`, () => {
    const host: HostPlugin = { media: { hosts: [`10.0.0.5`] }, urls: { hosts: [`10.0.0.5`] } }
    const c = paint({ id: `i`, component: `Image`, props: { src: `http://10.0.0.5/a.png`, alt: `a` } }, host)
    expect(c.querySelector(`img`)?.getAttribute(`src`) ?? null).toBeNull()
    cleanup()
    const l = paint({ id: `l`, component: `Link`, props: { href: `http://10.0.0.5/x`, label: `x`, external: true } }, host)
    expect(l.querySelector(`a`)!.getAttribute(`href`)).toBeNull()
  })
  it(`the page's own private origin (jsdom: localhost) is opted in, nothing else private is`, () => {
    expect(new URL(document.baseURI).hostname).toBe(`localhost`)
    const c = paint({ id: `i`, component: `Image`, props: { src: `x.png`, alt: `x` } })
    expect(c.querySelector(`img`)!.getAttribute(`src`)).toBe(`${new URL(document.baseURI).origin}/x.png`)
    cleanup()
    const l = paint({ id: `l`, component: `Link`, props: { href: `/docs`, label: `x` } })
    expect(l.querySelector(`a`)!.getAttribute(`href`)).toBe(`${new URL(document.baseURI).origin}/docs`)
    cleanup()
    const other = paint({ id: `i`, component: `Image`, props: { src: `http://127.0.0.1:3000/x.png`, alt: `x` } })
    expect(other.querySelector(`img`)?.getAttribute(`src`) ?? null).toBeNull()
  })
  it(`a host mediaRequest hook cannot widen the private rule`, () => {
    const host: HostPlugin = { mediaRequest: (src) => ({ url: src, headers: { authorization: `Bearer t` } }) }
    const c = paint({ id: `i`, component: `Image`, props: { src: `http://169.254.169.254/latest/a.png`, alt: `a` } }, host)
    expect(c.querySelector(`img`)?.getAttribute(`src`) ?? null).toBeNull()
  })
  it(`a markdown link or image to a private host is its label / alt`, () => {
    const c = markdown(`see [router](http://192.168.0.1/) ![cam](http://10.0.0.9/cam.jpg) end`)
    expect(c.querySelector(`a`)).toBeNull()
    expect(c.querySelector(`img`)).toBeNull()
    expect(c.textContent).toContain(`router`)
    expect(c.textContent).toContain(`cam end`)
    expect(c.innerHTML).not.toContain(`192.168`)
    expect(c.innerHTML).not.toContain(`10.0.0.9`)
  })
})

describe(`VAPP-107: schemes: ["https"] is https only`, () => {
  const host: HostPlugin = { urls: { schemes: [`https`] }, media: { schemes: [`https`] } }
  it(`an http Link paints as text, an https one navigates; mailto drops too`, () => {
    const c = paint({ id: `l`, component: `Link`, props: { href: `http://exponential.at/x`, label: `x`, external: true } }, host)
    expect(c.querySelector(`a`)!.getAttribute(`href`)).toBeNull()
    cleanup()
    const ok = paint({ id: `l`, component: `Link`, props: { href: `https://exponential.at/x`, label: `x`, external: true } }, host)
    expect(ok.querySelector(`a`)!.getAttribute(`href`)).toBe(`https://exponential.at/x`)
    cleanup()
    const mail = paint({ id: `l`, component: `Link`, props: { href: `mailto:a@b.c`, label: `x` } }, host)
    expect(mail.querySelector(`a`)!.getAttribute(`href`)).toBeNull()
  })
  it(`an http Image src loads nothing, an https one loads; data: drops too`, () => {
    const c = paint({ id: `i`, component: `Image`, props: { src: `http://exponential.at/a.png`, alt: `a` } }, host)
    expect(c.querySelector(`img`)?.getAttribute(`src`) ?? null).toBeNull()
    cleanup()
    const ok = paint({ id: `i`, component: `Image`, props: { src: `https://exponential.at/a.png`, alt: `a` } }, host)
    expect(ok.querySelector(`img`)!.getAttribute(`src`)).toBe(`https://exponential.at/a.png`)
    cleanup()
    const data = paint({ id: `i`, component: `Image`, props: { src: `data:image/png;base64,iVBORw0KGgo=`, alt: `a` } }, host)
    expect(data.querySelector(`img`)?.getAttribute(`src`) ?? null).toBeNull()
  })
})

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
  it(`Video/Audio with headers skip the pixel check but keep the byte cap (VAPP-107); the poster keeps the image limits`, async () => {
    clearMediaCache()
    let n = 0
    vi.spyOn(URL, `createObjectURL`).mockImplementation(() => `blob:media-${++n}`)
    vi.spyOn(URL, `revokeObjectURL`).mockImplementation(() => {})
    // 8 bytes that are no image: a stream takes them, an image (the poster) refuses them as an unreadable raster.
    vi.spyOn(globalThis, `fetch`).mockImplementation(async (u) =>
      String(u).endsWith(`poster.png`)
        ? new Response(new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), { status: 200, headers: { "content-type": `image/png` } })
        : new Response(new Uint8Array(8), { status: 200, headers: { "content-type": `video/mp4` } })
    )
    const host: HostPlugin = { mediaRequest: (src) => ({ url: src, headers: { authorization: `Bearer t` } }) }
    const c = paint({ id: `v`, component: `Video`, props: { src: `https://exponential.at/clip.mp4`, poster: `https://exponential.at/poster.png` } }, host)
    await vi.waitFor(() => expect(c.querySelector(`video`)!.getAttribute(`src`)).toMatch(/^blob:media-/))
    await new Promise((r) => setTimeout(r, 20))
    expect(c.querySelector(`video`)!.getAttribute(`poster`)).toBeNull()
    cleanup()
    const a = paint({ id: `a`, component: `AudioPlayer`, props: { src: `https://exponential.at/talk.m4a` } }, host)
    await vi.waitFor(() => expect(a.querySelector(`audio`)!.getAttribute(`src`)).toMatch(/^blob:media-/))
    cleanup()
    // Over the byte cap, as a whole film is: the whole-fetch fallback fails like a 404 (the host wants mediaStreamUrl).
    vi.spyOn(globalThis, `fetch`).mockImplementation(async () => new Response(new Uint8Array(8), { status: 200, headers: { "content-length": String(MEDIA_LIMITS.maxBytes + 1) } }))
    const big = paint({ id: `v`, component: `Video`, props: { src: `https://exponential.at/film.mp4` } }, host)
    await new Promise((r) => setTimeout(r, 20))
    expect(big.querySelector(`video`)!.getAttribute(`src`)).toBeNull()
  })
  it(`VAPP-107: the video/audio whole-fetch fallback keeps the byte cap and a total timeout`, async () => {
    const body = (n: number) =>
      new ReadableStream<Uint8Array>({
        start(c) {
          c.enqueue(new Uint8Array(n))
          c.close()
        },
      })
    vi.spyOn(globalThis, `fetch`).mockImplementation(async (u) => new Response(String(u).endsWith(`big.mp4`) ? body(150) : body(50), { status: 200, headers: { "content-type": `video/mp4` } }))
    await expect(fetchStream(`https://x.test/big.mp4`, {}, { maxBytes: 100, timeoutMs: 1000 })).rejects.toThrow(/over 100 bytes/)
    const ok = await fetchStream(`https://x.test/clip.mp4`, {}, { maxBytes: 100, timeoutMs: 1000 })
    expect(ok.size).toBe(50)
    expect(ok.type).toBe(`video/mp4`)
    // A download that outlives timeoutMs (the headers came, the body never ends) is aborted.
    vi.spyOn(globalThis, `fetch`).mockImplementation(async (_u, init) => {
      const signal = init?.signal as AbortSignal
      return new Response(new ReadableStream<Uint8Array>({ start: (c) => signal.addEventListener(`abort`, () => c.error(new Error(`aborted mid-body`))) }), { status: 200 })
    })
    await expect(fetchStream(`https://x.test/endless.mp4`, {}, { maxBytes: 100, timeoutMs: 10 })).rejects.toThrow(/aborted mid-body/)
    // An over-long Content-Length is refused before any byte is read.
    vi.spyOn(globalThis, `fetch`).mockImplementation(async () => new Response(body(1), { status: 200, headers: { "content-length": `999` } }))
    await expect(fetchStream(`https://x.test/long.mp4`, {}, { maxBytes: 100, timeoutMs: 1000 })).rejects.toThrow(/over 100 bytes/)
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
